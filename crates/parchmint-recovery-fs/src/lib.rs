//! Native, project-local recovery storage.

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use parchmint_contracts::generated::RecoveryRecordV1;
use parchmint_history_api::{
    CanonicalRelativePath, CheckpointCategory, CheckpointInput, CheckpointIntentHash, SnapshotName,
};
use parchmint_recovery_api::{
    CompactionReport, DiscardReport, DocumentId, DocumentRevision, DurableRevisionVector,
    EditorRevisionRange, RecoveryBaseSnapshot, RecoveryBatch, RecoveryError, RecoveryInventory,
    RecoveryJournal, RecoveryReceipt, RecoveryRecord, RecoveryRecordSummary, RecoveryReplay,
    RecoveryRevisionVector, ResourceId, VersionedRecoveryPayload, is_covered_by_durable,
    replay_records,
};
use parchmint_save::{
    AtomicWritePlan, CheckpointIntent, CheckpointIntentState, CheckpointIntentStore,
    CheckpointReceipt, CommitReceipt, ContentHash, IntentStoreError, ProjectId, ProjectRevision,
    ResourceRevision, SaveGeneration, SavePriority, SaveRevisionVector,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const RECOVERY_DIR: [&str; 2] = [".parchmint", "recovery"];
const JOURNAL_FILE: &str = "journal.bin";
const INTENTS_FILE: &str = "checkpoint-intents.bin";
const JOURNAL_MAGIC: &[u8; 4] = b"PMRJ";
const INTENTS_MAGIC: &[u8; 4] = b"PMCI";
const FRAME_VERSION: u16 = 1;
const JOURNAL_HEADER_LEN: usize = 56;
const SNAPSHOT_HEADER_LEN: usize = 48;
const MAX_RECORD_BYTES: usize = 64 * 1024 * 1024;
const INTENT_STORE_VERSION: u32 = 1;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Native recovery storage for one project.
///
/// Share one instance per open project to serialize journal and intent updates.
#[derive(Debug)]
pub struct FsRecoveryJournal {
    root: PathBuf,
    root_identity: FileIdentity,
    operations: Mutex<()>,
}

impl FsRecoveryJournal {
    /// Opens or creates recovery storage beneath an existing project directory.
    pub fn open(project_root: impl AsRef<Path>) -> Result<Self, RecoveryError> {
        let requested = project_root.as_ref();
        reject_lexical_escape(requested).map_err(|error| recovery_io("open root", error))?;
        let project_metadata = fs::symlink_metadata(requested)
            .map_err(|error| recovery_io("inspect project root", error))?;
        if !project_metadata.is_dir() || project_metadata.file_type().is_symlink() {
            return Err(unsafe_recovery_path(requested));
        }
        let project = fs::canonicalize(requested)
            .map_err(|error| recovery_io("canonicalize project root", error))?;

        let metadata_dir = ensure_checked_directory(&project, RECOVERY_DIR[0])?;
        let recovery = ensure_checked_directory(&metadata_dir, RECOVERY_DIR[1])?;
        let canonical_recovery = fs::canonicalize(&recovery)
            .map_err(|error| recovery_io("canonicalize recovery root", error))?;
        if canonical_recovery != recovery || !canonical_recovery.starts_with(&project) {
            return Err(unsafe_recovery_path(&recovery));
        }

        let journal = Self {
            root_identity: file_identity(&canonical_recovery)
                .map_err(|error| recovery_io("identify recovery root", error))?,
            root: canonical_recovery,
            operations: Mutex::new(()),
        };
        journal.reconcile_replacement(JOURNAL_FILE)?;
        journal.reconcile_replacement(INTENTS_FILE)?;
        journal.ensure_file(JOURNAL_FILE, &[])?;
        let empty_intents = encode_intent_snapshot(&IntentState::default()).map_err(|reason| {
            RecoveryError::Storage {
                operation: "initialize checkpoint intents",
                reason,
            }
        })?;
        journal.ensure_file(INTENTS_FILE, &empty_intents)?;
        Ok(journal)
    }

    fn ensure_file(&self, name: &str, initial: &[u8]) -> Result<(), RecoveryError> {
        self.verify_root()?;
        let path = self.root.join(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(()),
            Ok(_) => Err(unsafe_recovery_path(&path)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|error| recovery_io("create recovery file", error))?;
                file.write_all(initial)
                    .map_err(|error| recovery_io("initialize recovery file", error))?;
                file.sync_all()
                    .map_err(|error| recovery_io("flush recovery file", error))?;
                sync_directory(&self.root)
                    .map_err(|error| recovery_io("flush recovery directory", error))
            }
            Err(error) => Err(recovery_io("inspect recovery file", error)),
        }
    }

    fn verify_root(&self) -> Result<(), RecoveryError> {
        let metadata = fs::symlink_metadata(&self.root)
            .map_err(|error| recovery_io("inspect recovery root", error))?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || file_identity_from_metadata(&metadata) != self.root_identity
        {
            return Err(unsafe_recovery_path(&self.root));
        }
        let canonical = fs::canonicalize(&self.root)
            .map_err(|error| recovery_io("canonicalize recovery root", error))?;
        if canonical != self.root {
            return Err(unsafe_recovery_path(&self.root));
        }
        Ok(())
    }

    fn checked_file(&self, name: &str) -> Result<PathBuf, RecoveryError> {
        self.verify_root()?;
        let path = self.root.join(name);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| recovery_io("inspect recovery file", error))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(unsafe_recovery_path(&path));
        }
        Ok(path)
    }

    fn read_file(&self, name: &str, operation: &'static str) -> Result<Vec<u8>, RecoveryError> {
        let path = self.checked_file(name)?;
        let before = file_identity(&path).map_err(|error| recovery_io(operation, error))?;
        let mut file = File::open(&path).map_err(|error| recovery_io(operation, error))?;
        if file_identity_from_metadata(
            &file
                .metadata()
                .map_err(|error| recovery_io(operation, error))?,
        ) != before
        {
            return Err(unsafe_recovery_path(&path));
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|error| recovery_io(operation, error))?;
        if file_identity(&path).map_err(|error| recovery_io(operation, error))? != before {
            return Err(unsafe_recovery_path(&path));
        }
        Ok(bytes)
    }

    fn read_journal(&self) -> Result<Vec<StoredFrame>, RecoveryError> {
        Ok(decode_journal(
            &self.read_file(JOURNAL_FILE, "read journal")?,
        ))
    }

    fn append_frame(&self, bytes: &[u8]) -> Result<(), RecoveryError> {
        let path = self.checked_file(JOURNAL_FILE)?;
        let before = file_identity(&path)
            .map_err(|error| recovery_io("identify journal before append", error))?;
        let mut file = OpenOptions::new()
            .append(true)
            .open(&path)
            .map_err(|error| recovery_io("open journal for append", error))?;
        if file_identity_from_metadata(
            &file
                .metadata()
                .map_err(|error| recovery_io("identify open journal", error))?,
        ) != before
        {
            return Err(unsafe_recovery_path(&path));
        }
        file.write_all(bytes)
            .map_err(|error| recovery_io("append journal", error))?;
        file.sync_all()
            .map_err(|error| recovery_io("flush journal", error))?;
        if file_identity(&path)
            .map_err(|error| recovery_io("recheck journal after append", error))?
            != before
        {
            return Err(unsafe_recovery_path(&path));
        }
        Ok(())
    }

    fn sync_journal(&self) -> Result<(), RecoveryError> {
        let path = self.checked_file(JOURNAL_FILE)?;
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .and_then(|file| file.sync_all())
            .map_err(|error| recovery_io("flush journal", error))
    }

    fn sync_intents(&self) -> Result<(), IntentStoreError> {
        let path = self
            .checked_file(INTENTS_FILE)
            .map_err(recovery_to_intent)?;
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .and_then(|file| file.sync_all())
            .and_then(|()| sync_directory(&self.root))
            .map_err(|error| IntentStoreError::Storage {
                operation: "flush checkpoint intents",
                reason: error.to_string(),
            })
    }

    fn replace_file(
        &self,
        name: &str,
        bytes: &[u8],
        operation: &'static str,
    ) -> Result<(), RecoveryError> {
        self.reconcile_replacement(name)?;
        let target = self.checked_file(name)?;
        let target_identity =
            file_identity(&target).map_err(|error| recovery_io(operation, error))?;
        let (temporary, mut file) = self.create_temporary(name, operation)?;
        let result = (|| {
            file.write_all(bytes)
                .map_err(|error| recovery_io(operation, error))?;
            file.sync_all()
                .map_err(|error| recovery_io(operation, error))?;
            drop(file);
            self.verify_root()?;
            if file_identity(&target).map_err(|error| recovery_io(operation, error))?
                != target_identity
            {
                return Err(unsafe_recovery_path(&target));
            }
            replace_path(&temporary, &target, &replacement_backup(&target))
                .map_err(|error| recovery_io(operation, error))?;
            sync_directory(&self.root).map_err(|error| recovery_io(operation, error))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn reconcile_replacement(&self, name: &str) -> Result<(), RecoveryError> {
        self.verify_root()?;
        let target = self.root.join(name);
        let backup = replacement_backup(&target);
        let target_exists = checked_regular_file_exists(&target)?;
        let backup_exists = checked_regular_file_exists(&backup)?;
        match (target_exists, backup_exists) {
            (false, true) => {
                fs::rename(&backup, &target)
                    .map_err(|error| recovery_io("restore interrupted replacement", error))?;
                sync_directory(&self.root)
                    .map_err(|error| recovery_io("flush restored replacement", error))
            }
            (true, true) => {
                fs::remove_file(&backup)
                    .map_err(|error| recovery_io("remove completed replacement backup", error))?;
                sync_directory(&self.root)
                    .map_err(|error| recovery_io("flush replacement cleanup", error))
            }
            _ => Ok(()),
        }
    }

    fn create_temporary(
        &self,
        name: &str,
        operation: &'static str,
    ) -> Result<(PathBuf, File), RecoveryError> {
        for _ in 0..32 {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = self
                .root
                .join(format!(".{name}.{}.{sequence}.tmp", std::process::id()));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok((path, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(recovery_io(operation, error)),
            }
        }
        Err(RecoveryError::Storage {
            operation,
            reason: "could not allocate a unique temporary recovery file".into(),
        })
    }

    fn compact_locked(
        &self,
        durable: &DurableRevisionVector,
    ) -> Result<CompactionReport, RecoveryError> {
        let frames = self.read_journal()?;
        validated_batches(&frames)?;
        let mut retained = Vec::new();
        let mut removed_records = 0;
        for frame in frames {
            if is_covered_by_durable(&frame.record, durable) {
                removed_records += 1;
            } else {
                retained.extend_from_slice(&frame.raw);
            }
        }
        let retained_records = validated_record_count(&retained)?;
        if removed_records > 0 {
            self.replace_file(JOURNAL_FILE, &retained, "compact journal")?;
        }
        Ok(CompactionReport {
            removed_records,
            retained_records,
        })
    }

    fn load_intents(&self) -> Result<IntentState, IntentStoreError> {
        let bytes = self
            .read_file(INTENTS_FILE, "read checkpoint intents")
            .map_err(recovery_to_intent)?;
        decode_intent_snapshot(&bytes).map_err(|reason| IntentStoreError::Storage {
            operation: "decode checkpoint intents",
            reason,
        })
    }

    fn store_intents(&self, state: &IntentState) -> Result<(), IntentStoreError> {
        let bytes = encode_intent_snapshot(state).map_err(|reason| IntentStoreError::Storage {
            operation: "encode checkpoint intents",
            reason,
        })?;
        self.replace_file(INTENTS_FILE, &bytes, "persist checkpoint intents")
            .map_err(recovery_to_intent)
    }
}

impl RecoveryJournal for FsRecoveryJournal {
    fn append(&self, batch: RecoveryBatch) -> Result<RecoveryReceipt, RecoveryError> {
        let _guard = self
            .operations
            .lock()
            .map_err(|_| poisoned_recovery_lock())?;
        let frames = self.read_journal()?;
        let batches = validated_batches(&frames)?;
        if batches.last() == Some(&batch) {
            self.sync_journal()?;
            return Ok(RecoveryReceipt::for_batch(&batch));
        }
        batch.validate_after(batches.last())?;
        let frame = encode_journal_frame(&batch)?;
        self.append_frame(&frame)?;
        Ok(RecoveryReceipt::for_batch(&batch))
    }

    fn flush_through(
        &self,
        target: RecoveryRevisionVector,
    ) -> Result<RecoveryReceipt, RecoveryError> {
        let _guard = self
            .operations
            .lock()
            .map_err(|_| poisoned_recovery_lock())?;
        let frames = self.read_journal()?;
        validated_batches(&frames)?;
        let records = frames
            .iter()
            .map(|frame| frame.record.clone())
            .collect::<Vec<_>>();
        let batch = records
            .iter()
            .find_map(|record| match record {
                RecoveryRecord::Complete(batch) if batch.revision_vector() == target => Some(batch),
                _ => None,
            })
            .ok_or(RecoveryError::UnknownRevisionVector)?;
        self.sync_journal()?;
        Ok(RecoveryReceipt::for_batch(batch))
    }

    fn inspect(&self) -> Result<RecoveryInventory, RecoveryError> {
        let _guard = self
            .operations
            .lock()
            .map_err(|_| poisoned_recovery_lock())?;
        let frames = self.read_journal()?;
        let mut previous = None;
        let mut durable_through = None;
        for frame in &frames {
            let RecoveryRecord::Complete(batch) = &frame.record else {
                break;
            };
            if batch.validate_after(previous).is_err() {
                break;
            }
            durable_through = Some(batch.revision_vector());
            previous = Some(batch);
        }
        Ok(RecoveryInventory {
            records: frames
                .iter()
                .enumerate()
                .map(|(position, frame)| RecoveryRecordSummary {
                    position,
                    project_revision: record_project_revision(&frame.record),
                })
                .collect(),
            durable_through,
        })
    }

    fn replay(&self, base: RecoveryBaseSnapshot) -> Result<RecoveryReplay, RecoveryError> {
        let _guard = self
            .operations
            .lock()
            .map_err(|_| poisoned_recovery_lock())?;
        let records = self
            .read_journal()?
            .into_iter()
            .map(|frame| frame.record)
            .collect::<Vec<_>>();
        Ok(replay_records(&base, records))
    }

    fn compact(&self, durable: DurableRevisionVector) -> Result<CompactionReport, RecoveryError> {
        let _guard = self
            .operations
            .lock()
            .map_err(|_| poisoned_recovery_lock())?;
        self.compact_locked(&durable)
    }

    fn discard_through(
        &self,
        durable: DurableRevisionVector,
    ) -> Result<DiscardReport, RecoveryError> {
        let _guard = self
            .operations
            .lock()
            .map_err(|_| poisoned_recovery_lock())?;
        let report = self.compact_locked(&durable)?;
        Ok(DiscardReport {
            removed_records: report.removed_records,
            retained_records: report.retained_records,
        })
    }
}

impl CheckpointIntentStore for FsRecoveryJournal {
    fn persist(&self, intent: CheckpointIntent) -> Result<(), IntentStoreError> {
        let _guard = self.operations.lock().map_err(|_| poisoned_intent_lock())?;
        let hash = intent.intent_hash();
        let mut state = self.load_intents()?;
        if let Some(completed) = state.completed.get(&hash) {
            if completed.project == intent.project && completed.revisions == intent.revisions {
                return self.sync_intents();
            }
            return Err(IntentStoreError::Conflict { intent_hash: hash });
        }
        if let Some(existing) = state.pending.get(&hash) {
            if existing.project != intent.project
                || existing.revisions != intent.revisions
                || existing.writes != intent.writes
                || existing.checkpoint != intent.checkpoint
                || existing.priority != intent.priority
            {
                return Err(IntentStoreError::Conflict { intent_hash: hash });
            }
            match (&existing.state, &intent.state) {
                (
                    CheckpointIntentState::FilesCommitted { receipt: existing },
                    CheckpointIntentState::FilesCommitted { receipt: incoming },
                ) if existing != incoming => {
                    return Err(IntentStoreError::Conflict { intent_hash: hash });
                }
                (CheckpointIntentState::FilesCommitted { .. }, CheckpointIntentState::Planned) => {
                    return self.sync_intents();
                }
                _ if existing == &intent => return self.sync_intents(),
                _ => {}
            }
        }
        state.pending.insert(hash, intent);
        self.store_intents(&state)
    }

    fn pending(&self) -> Result<Vec<CheckpointIntent>, IntentStoreError> {
        let _guard = self.operations.lock().map_err(|_| poisoned_intent_lock())?;
        Ok(self.load_intents()?.pending.into_values().collect())
    }

    fn complete(&self, receipt: CheckpointReceipt) -> Result<(), IntentStoreError> {
        let _guard = self.operations.lock().map_err(|_| poisoned_intent_lock())?;
        let mut state = self.load_intents()?;
        if let Some(completed) = state.completed.get(&receipt.intent_hash) {
            if completed == &receipt {
                return self.sync_intents();
            }
            return Err(IntentStoreError::Conflict {
                intent_hash: receipt.intent_hash,
            });
        }
        let matches = state
            .pending
            .get(&receipt.intent_hash)
            .is_some_and(|intent| {
                intent.project == receipt.project
                    && intent.revisions == receipt.revisions
                    && matches!(intent.state, CheckpointIntentState::FilesCommitted { .. })
            });
        if !matches {
            return Err(IntentStoreError::Conflict {
                intent_hash: receipt.intent_hash,
            });
        }
        state.pending.remove(&receipt.intent_hash);
        state.completed.insert(receipt.intent_hash, receipt);
        self.store_intents(&state)
    }
}

#[derive(Debug)]
struct StoredFrame {
    record: RecoveryRecord,
    raw: Vec<u8>,
}

fn encode_journal_frame(batch: &RecoveryBatch) -> Result<Vec<u8>, RecoveryError> {
    let payload = serde_json::to_vec(&WireRecoveryBatch::from(batch)).map_err(|error| {
        RecoveryError::Storage {
            operation: "encode journal record",
            reason: error.to_string(),
        }
    })?;
    if payload.len() > MAX_RECORD_BYTES {
        return Err(RecoveryError::Storage {
            operation: "encode journal record",
            reason: "record exceeds the maximum frame size".into(),
        });
    }
    let checksum = Sha256::digest(&payload);
    let mut frame = Vec::with_capacity(JOURNAL_HEADER_LEN + payload.len());
    frame.extend_from_slice(JOURNAL_MAGIC);
    frame.extend_from_slice(&FRAME_VERSION.to_le_bytes());
    frame.extend_from_slice(&0_u16.to_le_bytes());
    frame.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    frame.extend_from_slice(&batch.project_revision.value().to_le_bytes());
    frame.extend_from_slice(&checksum);
    frame.extend_from_slice(&payload);
    Ok(frame)
}

fn decode_journal(bytes: &[u8]) -> Vec<StoredFrame> {
    let mut frames = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let remaining = &bytes[offset..];
        if remaining.len() < JOURNAL_HEADER_LEN {
            frames.push(StoredFrame {
                record: RecoveryRecord::Truncated {
                    project_revision: partial_project_revision(remaining),
                },
                raw: remaining.to_vec(),
            });
            break;
        }
        let project_revision = Some(ProjectRevision::from(read_u64(&remaining[16..24])));
        if &remaining[..4] != JOURNAL_MAGIC {
            frames.push(StoredFrame {
                record: RecoveryRecord::Mismatched {
                    project_revision,
                    reason: "frame magic failed".into(),
                },
                raw: remaining.to_vec(),
            });
            break;
        }
        let payload_len_u64 = read_u64(&remaining[8..16]);
        let Ok(payload_len) = usize::try_from(payload_len_u64) else {
            frames.push(oversized_frame(project_revision, remaining));
            break;
        };
        if payload_len > MAX_RECORD_BYTES {
            frames.push(oversized_frame(project_revision, remaining));
            break;
        }
        let Some(frame_len) = JOURNAL_HEADER_LEN.checked_add(payload_len) else {
            frames.push(oversized_frame(project_revision, remaining));
            break;
        };
        if remaining.len() < frame_len {
            frames.push(StoredFrame {
                record: RecoveryRecord::Truncated { project_revision },
                raw: remaining.to_vec(),
            });
            break;
        }
        let raw = remaining[..frame_len].to_vec();
        let payload = &remaining[JOURNAL_HEADER_LEN..frame_len];
        let version = read_u16(&remaining[4..6]);
        let record = if version != FRAME_VERSION {
            RecoveryRecord::UnknownVersion {
                project_revision,
                version: format!("parchmint.recovery-frame/v{version}"),
            }
        } else if remaining[24..56] != Sha256::digest(payload)[..] {
            RecoveryRecord::Mismatched {
                project_revision,
                reason: "frame checksum failed".into(),
            }
        } else {
            decode_recovery_payload(payload, project_revision)
        };
        frames.push(StoredFrame { record, raw });
        offset += frame_len;
    }
    frames
}

fn decode_recovery_payload(
    payload: &[u8],
    framed_revision: Option<ProjectRevision>,
) -> RecoveryRecord {
    let wire = match serde_json::from_slice::<WireRecoveryBatch>(payload) {
        Ok(wire) => wire,
        Err(error) => {
            return RecoveryRecord::Mismatched {
                project_revision: framed_revision,
                reason: format!("record payload failed to decode: {error}"),
            };
        }
    };
    if Some(ProjectRevision::from(wire.project_revision)) != framed_revision {
        return RecoveryRecord::Mismatched {
            project_revision: framed_revision,
            reason: "frame and payload revisions differ".into(),
        };
    }
    if wire.payload.schema != "parchmint.recovery-record/v1" {
        return RecoveryRecord::UnknownVersion {
            project_revision: framed_revision,
            version: wire.payload.schema,
        };
    }
    match RecoveryBatch::try_from(wire) {
        Ok(batch) => RecoveryRecord::Complete(batch),
        Err(reason) => RecoveryRecord::Mismatched {
            project_revision: framed_revision,
            reason,
        },
    }
}

fn oversized_frame(project_revision: Option<ProjectRevision>, raw: &[u8]) -> StoredFrame {
    StoredFrame {
        record: RecoveryRecord::Mismatched {
            project_revision,
            reason: "frame length exceeds the supported limit".into(),
        },
        raw: raw.to_vec(),
    }
}

fn partial_project_revision(bytes: &[u8]) -> Option<ProjectRevision> {
    (bytes.len() >= 24).then(|| ProjectRevision::from(read_u64(&bytes[16..24])))
}

fn validated_batches(frames: &[StoredFrame]) -> Result<Vec<RecoveryBatch>, RecoveryError> {
    let mut batches = Vec::with_capacity(frames.len());
    for frame in frames {
        let RecoveryRecord::Complete(batch) = &frame.record else {
            return Err(RecoveryError::Storage {
                operation: "validate journal",
                reason: "journal contains a quarantined recovery frame".into(),
            });
        };
        batch.validate_after(batches.last())?;
        batches.push(batch.clone());
    }
    Ok(batches)
}

fn validated_record_count(bytes: &[u8]) -> Result<usize, RecoveryError> {
    let frames = decode_journal(bytes);
    validated_batches(&frames)?;
    Ok(frames.len())
}

fn record_project_revision(record: &RecoveryRecord) -> Option<ProjectRevision> {
    match record {
        RecoveryRecord::Complete(batch) => Some(batch.project_revision),
        RecoveryRecord::UnknownVersion {
            project_revision, ..
        }
        | RecoveryRecord::Truncated { project_revision }
        | RecoveryRecord::Mismatched {
            project_revision, ..
        }
        | RecoveryRecord::Ambiguous { project_revision } => *project_revision,
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRecoveryBatch {
    project_revision: u64,
    documents: Vec<WireDocumentRange>,
    base_hashes: Vec<WireResourceHash>,
    result_hashes: Vec<WireResourceHash>,
    payload: RecoveryRecordV1,
}

impl From<&RecoveryBatch> for WireRecoveryBatch {
    fn from(batch: &RecoveryBatch) -> Self {
        let VersionedRecoveryPayload::V1(payload) = &batch.payload;
        Self {
            project_revision: batch.project_revision.value(),
            documents: batch
                .documents
                .iter()
                .map(|(document, range)| WireDocumentRange {
                    document: *document.as_bytes(),
                    first: range.first.value(),
                    last: range.last.value(),
                })
                .collect(),
            base_hashes: wire_hashes(&batch.base_hashes),
            result_hashes: wire_hashes(&batch.result_hashes),
            payload: payload.clone(),
        }
    }
}

impl TryFrom<WireRecoveryBatch> for RecoveryBatch {
    type Error = String;

    fn try_from(wire: WireRecoveryBatch) -> Result<Self, Self::Error> {
        let mut documents = BTreeMap::new();
        for range in wire.documents {
            let document = DocumentId::from_bytes(range.document);
            let range = EditorRevisionRange::new(range.first.into(), range.last.into())
                .map_err(|error| error.to_string())?;
            if documents.insert(document, range).is_some() {
                return Err("record repeats a document revision range".into());
            }
        }
        Ok(Self {
            project_revision: wire.project_revision.into(),
            documents,
            base_hashes: recovery_hashes(wire.base_hashes)?,
            result_hashes: recovery_hashes(wire.result_hashes)?,
            payload: VersionedRecoveryPayload::V1(wire.payload),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireDocumentRange {
    document: [u8; 16],
    first: u64,
    last: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireResourceHash {
    resource: WireResourceId,
    hash: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "document_id", rename_all = "kebab-case")]
enum WireResourceId {
    FormatControl,
    Manifest,
    Styles,
    Dictionary,
    Document,
    Annotations(String),
}

fn wire_resource(resource: &ResourceId) -> WireResourceId {
    match resource {
        ResourceId::FormatControl => WireResourceId::FormatControl,
        ResourceId::Manifest => WireResourceId::Manifest,
        ResourceId::Styles => WireResourceId::Styles,
        ResourceId::Dictionary => WireResourceId::Dictionary,
        ResourceId::Document => WireResourceId::Document,
        ResourceId::Annotations { document_id } => WireResourceId::Annotations(document_id.clone()),
    }
}

fn recovery_resource(resource: WireResourceId) -> ResourceId {
    match resource {
        WireResourceId::FormatControl => ResourceId::FormatControl,
        WireResourceId::Manifest => ResourceId::Manifest,
        WireResourceId::Styles => ResourceId::Styles,
        WireResourceId::Dictionary => ResourceId::Dictionary,
        WireResourceId::Document => ResourceId::Document,
        WireResourceId::Annotations(document_id) => ResourceId::Annotations { document_id },
    }
}

fn wire_hashes(hashes: &BTreeMap<ResourceId, ContentHash>) -> Vec<WireResourceHash> {
    hashes
        .iter()
        .map(|(resource, hash)| WireResourceHash {
            resource: wire_resource(resource),
            hash: *hash.as_bytes(),
        })
        .collect()
}

fn recovery_hashes(
    hashes: Vec<WireResourceHash>,
) -> Result<BTreeMap<ResourceId, ContentHash>, String> {
    let mut decoded = BTreeMap::new();
    for hash in hashes {
        if decoded
            .insert(
                recovery_resource(hash.resource),
                ContentHash::from_bytes(hash.hash),
            )
            .is_some()
        {
            return Err("record repeats a resource hash".into());
        }
    }
    Ok(decoded)
}

#[derive(Debug, Default)]
struct IntentState {
    pending: BTreeMap<CheckpointIntentHash, CheckpointIntent>,
    completed: BTreeMap<CheckpointIntentHash, CheckpointReceipt>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireIntentState {
    version: u32,
    pending: Vec<WireCheckpointIntent>,
    completed: Vec<WireCheckpointReceipt>,
}

impl From<&IntentState> for WireIntentState {
    fn from(state: &IntentState) -> Self {
        Self {
            version: INTENT_STORE_VERSION,
            pending: state
                .pending
                .values()
                .map(WireCheckpointIntent::from)
                .collect(),
            completed: state
                .completed
                .values()
                .map(WireCheckpointReceipt::from)
                .collect(),
        }
    }
}

impl TryFrom<WireIntentState> for IntentState {
    type Error = String;

    fn try_from(wire: WireIntentState) -> Result<Self, Self::Error> {
        if wire.version != INTENT_STORE_VERSION {
            return Err(format!(
                "unsupported checkpoint intent version {}",
                wire.version
            ));
        }
        let mut state = Self::default();
        for wire_intent in wire.pending {
            let intent = CheckpointIntent::try_from(wire_intent)?;
            let hash = intent.intent_hash();
            if state.pending.insert(hash, intent).is_some() {
                return Err("checkpoint intent appears more than once".into());
            }
        }
        for wire_receipt in wire.completed {
            let receipt = CheckpointReceipt::try_from(wire_receipt)?;
            if state.pending.contains_key(&receipt.intent_hash)
                || state
                    .completed
                    .insert(receipt.intent_hash, receipt)
                    .is_some()
            {
                return Err("checkpoint intent has ambiguous durable state".into());
            }
        }
        Ok(state)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCheckpointIntent {
    project: [u8; 16],
    revisions: WireSaveRevisionVector,
    writes: Vec<WireStagedResource>,
    checkpoint: WireCheckpointInput,
    priority: WireSavePriority,
    state: WireCheckpointIntentState,
}

impl From<&CheckpointIntent> for WireCheckpointIntent {
    fn from(intent: &CheckpointIntent) -> Self {
        Self {
            project: *intent.project.as_bytes(),
            revisions: WireSaveRevisionVector::from(&intent.revisions),
            writes: intent
                .writes
                .writes
                .iter()
                .map(|write| WireStagedResource {
                    path: write.path.clone(),
                    bytes: write.bytes.clone(),
                })
                .collect(),
            checkpoint: WireCheckpointInput::from(&intent.checkpoint),
            priority: WireSavePriority::from(intent.priority),
            state: WireCheckpointIntentState::from(&intent.state),
        }
    }
}

impl TryFrom<WireCheckpointIntent> for CheckpointIntent {
    type Error = String;

    fn try_from(wire: WireCheckpointIntent) -> Result<Self, Self::Error> {
        let checkpoint = CheckpointInput::try_from(wire.checkpoint)?;
        checkpoint.validate().map_err(|error| error.to_string())?;
        let intent = Self {
            project: ProjectId::from_bytes(wire.project),
            revisions: SaveRevisionVector::try_from(wire.revisions)?,
            writes: AtomicWritePlan::new(
                wire.writes
                    .into_iter()
                    .map(|write| parchmint_history_api::StagedResource {
                        path: write.path,
                        bytes: write.bytes,
                    })
                    .collect(),
            ),
            checkpoint,
            priority: SavePriority::from(wire.priority),
            state: CheckpointIntentState::from(wire.state),
        };
        validate_write_paths(&intent.writes)?;
        Ok(intent)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireStagedResource {
    path: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSaveRevisionVector {
    project_revision: u64,
    open_documents: Vec<WireDocumentRevision>,
    closed_resources: Vec<WireResourceRevision>,
    canonical_hashes: Vec<WireResourceHash>,
    generation: u64,
}

impl From<&SaveRevisionVector> for WireSaveRevisionVector {
    fn from(revisions: &SaveRevisionVector) -> Self {
        Self {
            project_revision: revisions.project_revision.value(),
            open_documents: revisions
                .open_documents
                .iter()
                .map(|(document, revision)| WireDocumentRevision {
                    document: *document.as_bytes(),
                    revision: revision.value(),
                })
                .collect(),
            closed_resources: revisions
                .closed_resources
                .iter()
                .map(|(resource, revision)| WireResourceRevision {
                    resource: wire_resource(resource),
                    revision: revision.value(),
                })
                .collect(),
            canonical_hashes: wire_hashes(&revisions.canonical_hashes),
            generation: revisions.generation.value(),
        }
    }
}

impl TryFrom<WireSaveRevisionVector> for SaveRevisionVector {
    type Error = String;

    fn try_from(wire: WireSaveRevisionVector) -> Result<Self, Self::Error> {
        Ok(Self {
            project_revision: wire.project_revision.into(),
            open_documents: document_revisions(wire.open_documents)?,
            closed_resources: resource_revisions(wire.closed_resources)?,
            canonical_hashes: recovery_hashes(wire.canonical_hashes)?,
            generation: SaveGeneration::from(wire.generation),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireDocumentRevision {
    document: [u8; 16],
    revision: u64,
}

fn document_revisions(
    revisions: Vec<WireDocumentRevision>,
) -> Result<BTreeMap<DocumentId, DocumentRevision>, String> {
    let mut decoded = BTreeMap::new();
    for revision in revisions {
        if decoded
            .insert(
                DocumentId::from_bytes(revision.document),
                DocumentRevision::from(revision.revision),
            )
            .is_some()
        {
            return Err("save revisions repeat an open document".into());
        }
    }
    Ok(decoded)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireResourceRevision {
    resource: WireResourceId,
    revision: u64,
}

fn resource_revisions(
    revisions: Vec<WireResourceRevision>,
) -> Result<BTreeMap<ResourceId, ResourceRevision>, String> {
    let mut decoded = BTreeMap::new();
    for revision in revisions {
        if decoded
            .insert(
                recovery_resource(revision.resource),
                ResourceRevision::from(revision.revision),
            )
            .is_some()
        {
            return Err("save revisions repeat a closed resource".into());
        }
    }
    Ok(decoded)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCheckpointInput {
    intent_hash: [u8; 32],
    resources: Vec<WirePathHash>,
    category: WireCheckpointCategory,
    affected_documents: Vec<[u8; 16]>,
    name: Option<String>,
}

impl From<&CheckpointInput> for WireCheckpointInput {
    fn from(checkpoint: &CheckpointInput) -> Self {
        Self {
            intent_hash: *checkpoint.intent_hash.as_bytes(),
            resources: checkpoint
                .resources
                .iter()
                .map(|(path, hash)| WirePathHash {
                    path: path.as_str().to_owned(),
                    hash: *hash.as_bytes(),
                })
                .collect(),
            category: WireCheckpointCategory::from(checkpoint.category),
            affected_documents: checkpoint
                .affected_documents
                .iter()
                .map(|document| *document.as_bytes())
                .collect(),
            name: checkpoint
                .name
                .as_ref()
                .map(|name| name.as_str().to_owned()),
        }
    }
}

impl TryFrom<WireCheckpointInput> for CheckpointInput {
    type Error = String;

    fn try_from(wire: WireCheckpointInput) -> Result<Self, Self::Error> {
        let mut resources = BTreeMap::new();
        for resource in wire.resources {
            let path =
                CanonicalRelativePath::parse(&resource.path).map_err(|error| error.to_string())?;
            if resources
                .insert(path, ContentHash::from_bytes(resource.hash))
                .is_some()
            {
                return Err("checkpoint repeats a canonical resource path".into());
            }
        }
        let name = wire
            .name
            .map(SnapshotName::new)
            .transpose()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            intent_hash: CheckpointIntentHash::from_bytes(wire.intent_hash),
            resources,
            category: CheckpointCategory::from(wire.category),
            affected_documents: wire
                .affected_documents
                .into_iter()
                .map(DocumentId::from_bytes)
                .collect(),
            name,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WirePathHash {
    path: String,
    hash: [u8; 32],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum WireCheckpointCategory {
    Autosave,
    ExplicitSave,
    StructuralChange,
    NamedSnapshot,
    Restoration,
}

impl From<CheckpointCategory> for WireCheckpointCategory {
    fn from(category: CheckpointCategory) -> Self {
        match category {
            CheckpointCategory::Autosave => Self::Autosave,
            CheckpointCategory::ExplicitSave => Self::ExplicitSave,
            CheckpointCategory::StructuralChange => Self::StructuralChange,
            CheckpointCategory::NamedSnapshot => Self::NamedSnapshot,
            CheckpointCategory::Restoration => Self::Restoration,
        }
    }
}

impl From<WireCheckpointCategory> for CheckpointCategory {
    fn from(category: WireCheckpointCategory) -> Self {
        match category {
            WireCheckpointCategory::Autosave => Self::Autosave,
            WireCheckpointCategory::ExplicitSave => Self::ExplicitSave,
            WireCheckpointCategory::StructuralChange => Self::StructuralChange,
            WireCheckpointCategory::NamedSnapshot => Self::NamedSnapshot,
            WireCheckpointCategory::Restoration => Self::Restoration,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum WireSavePriority {
    Autosave,
    Structural,
    Explicit,
    Close,
}

impl From<SavePriority> for WireSavePriority {
    fn from(priority: SavePriority) -> Self {
        match priority {
            SavePriority::Autosave => Self::Autosave,
            SavePriority::Structural => Self::Structural,
            SavePriority::Explicit => Self::Explicit,
            SavePriority::Close => Self::Close,
        }
    }
}

impl From<WireSavePriority> for SavePriority {
    fn from(priority: WireSavePriority) -> Self {
        match priority {
            WireSavePriority::Autosave => Self::Autosave,
            WireSavePriority::Structural => Self::Structural,
            WireSavePriority::Explicit => Self::Explicit,
            WireSavePriority::Close => Self::Close,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "state", content = "receipt", rename_all = "kebab-case")]
enum WireCheckpointIntentState {
    Planned,
    FilesCommitted(u64),
}

impl From<&CheckpointIntentState> for WireCheckpointIntentState {
    fn from(state: &CheckpointIntentState) -> Self {
        match state {
            CheckpointIntentState::Planned => Self::Planned,
            CheckpointIntentState::FilesCommitted { receipt } => Self::FilesCommitted(receipt.id()),
        }
    }
}

impl From<WireCheckpointIntentState> for CheckpointIntentState {
    fn from(state: WireCheckpointIntentState) -> Self {
        match state {
            WireCheckpointIntentState::Planned => Self::Planned,
            WireCheckpointIntentState::FilesCommitted(receipt) => Self::FilesCommitted {
                receipt: CommitReceipt::new(receipt),
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCheckpointReceipt {
    project: [u8; 16],
    intent_hash: [u8; 32],
    checkpoint: [u8; 16],
    revisions: WireSaveRevisionVector,
}

impl From<&CheckpointReceipt> for WireCheckpointReceipt {
    fn from(receipt: &CheckpointReceipt) -> Self {
        Self {
            project: *receipt.project.as_bytes(),
            intent_hash: *receipt.intent_hash.as_bytes(),
            checkpoint: *receipt.checkpoint.as_bytes(),
            revisions: WireSaveRevisionVector::from(&receipt.revisions),
        }
    }
}

impl TryFrom<WireCheckpointReceipt> for CheckpointReceipt {
    type Error = String;

    fn try_from(wire: WireCheckpointReceipt) -> Result<Self, Self::Error> {
        Ok(Self {
            project: ProjectId::from_bytes(wire.project),
            intent_hash: CheckpointIntentHash::from_bytes(wire.intent_hash),
            checkpoint: parchmint_history_api::CheckpointId::from_bytes(wire.checkpoint),
            revisions: SaveRevisionVector::try_from(wire.revisions)?,
        })
    }
}

fn validate_write_paths(plan: &AtomicWritePlan) -> Result<(), String> {
    let mut paths = BTreeMap::new();
    for write in &plan.writes {
        let path = CanonicalRelativePath::parse(&write.path).map_err(|error| error.to_string())?;
        if paths.insert(path, ()).is_some() {
            return Err("checkpoint intent repeats a write path".into());
        }
    }
    Ok(())
}

fn encode_intent_snapshot(state: &IntentState) -> Result<Vec<u8>, String> {
    let payload =
        serde_json::to_vec(&WireIntentState::from(state)).map_err(|error| error.to_string())?;
    let checksum = Sha256::digest(&payload);
    let mut bytes = Vec::with_capacity(SNAPSHOT_HEADER_LEN + payload.len());
    bytes.extend_from_slice(INTENTS_MAGIC);
    bytes.extend_from_slice(&FRAME_VERSION.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&checksum);
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}

fn decode_intent_snapshot(bytes: &[u8]) -> Result<IntentState, String> {
    if bytes.len() < SNAPSHOT_HEADER_LEN {
        return Err("checkpoint intent snapshot is truncated".into());
    }
    if &bytes[..4] != INTENTS_MAGIC {
        return Err("checkpoint intent snapshot magic failed".into());
    }
    let version = read_u16(&bytes[4..6]);
    if version != FRAME_VERSION {
        return Err(format!(
            "unsupported checkpoint intent frame version {version}"
        ));
    }
    let payload_len = usize::try_from(read_u64(&bytes[8..16]))
        .map_err(|_| "checkpoint intent snapshot length is unsupported".to_owned())?;
    let expected_len = SNAPSHOT_HEADER_LEN
        .checked_add(payload_len)
        .ok_or_else(|| "checkpoint intent snapshot length overflowed".to_owned())?;
    if expected_len != bytes.len() {
        return Err("checkpoint intent snapshot length failed".into());
    }
    let payload = &bytes[SNAPSHOT_HEADER_LEN..];
    if bytes[16..48] != Sha256::digest(payload)[..] {
        return Err("checkpoint intent snapshot checksum failed".into());
    }
    let wire =
        serde_json::from_slice::<WireIntentState>(payload).map_err(|error| error.to_string())?;
    IntentState::try_from(wire)
}

fn reject_lexical_escape(path: &Path) -> io::Result<()> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsafe project path {}", path.display()),
        ));
    }
    Ok(())
}

fn ensure_checked_directory(parent: &Path, name: &str) -> Result<PathBuf, RecoveryError> {
    let path = parent.join(name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Err(unsafe_recovery_path(&path)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(&path)
                .map_err(|error| recovery_io("create recovery directory", error))?;
            sync_directory(parent)
                .map_err(|error| recovery_io("flush recovery directory parent", error))?;
        }
        Err(error) => return Err(recovery_io("inspect recovery directory", error)),
    }
    let canonical = fs::canonicalize(&path)
        .map_err(|error| recovery_io("canonicalize recovery directory", error))?;
    if canonical != path {
        return Err(unsafe_recovery_path(&path));
    }
    Ok(path)
}

fn checked_regular_file_exists(path: &Path) -> Result<bool, RecoveryError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(unsafe_recovery_path(path)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(recovery_io("inspect replacement path", error)),
    }
}

fn replacement_backup(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .expect("fixed recovery filenames are UTF-8");
    target.with_file_name(format!(".{name}.previous"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    platform_a: u64,
    platform_b: u64,
}

fn file_identity(path: &Path) -> io::Result<FileIdentity> {
    fs::symlink_metadata(path).map(|metadata| file_identity_from_metadata(&metadata))
}

#[cfg(unix)]
fn file_identity_from_metadata(metadata: &fs::Metadata) -> FileIdentity {
    use std::os::unix::fs::MetadataExt;
    FileIdentity {
        platform_a: metadata.dev(),
        platform_b: metadata.ino(),
    }
}

#[cfg(windows)]
fn file_identity_from_metadata(metadata: &fs::Metadata) -> FileIdentity {
    use std::os::windows::fs::MetadataExt;
    FileIdentity {
        platform_a: metadata.volume_serial_number().map_or(0, u64::from),
        platform_b: metadata.file_index().unwrap_or(0),
    }
}

#[cfg(all(not(unix), not(windows)))]
fn file_identity_from_metadata(_metadata: &fs::Metadata) -> FileIdentity {
    FileIdentity {
        platform_a: 0,
        platform_b: 0,
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(path: &Path) -> io::Result<()> {
    fs::metadata(path).map(|_| ())
}

#[cfg(not(windows))]
fn replace_path(source: &Path, target: &Path, _backup: &Path) -> io::Result<()> {
    fs::rename(source, target)
}

#[cfg(windows)]
fn replace_path(source: &Path, target: &Path, backup: &Path) -> io::Result<()> {
    match fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::AlreadyExists | io::ErrorKind::PermissionDenied
            ) =>
        {
            if fs::symlink_metadata(backup).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "recovery replacement backup already exists",
                ));
            }
            fs::rename(target, backup)?;
            sync_directory(target.parent().expect("recovery target has a parent"))?;
            if let Err(error) = fs::rename(source, target) {
                let _ = fs::rename(backup, target);
                let _ = sync_directory(target.parent().expect("recovery target has a parent"));
                return Err(error);
            }
            sync_directory(target.parent().expect("recovery target has a parent"))?;
            fs::remove_file(backup)?;
            sync_directory(target.parent().expect("recovery target has a parent"))
        }
        Err(error) => Err(error),
    }
}

fn read_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes(bytes.try_into().expect("two-byte frame field"))
}

fn read_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("eight-byte frame field"))
}

fn unsafe_recovery_path(path: &Path) -> RecoveryError {
    RecoveryError::Storage {
        operation: "check recovery path",
        reason: format!("unsafe recovery path {}", path.display()),
    }
}

fn recovery_io(operation: &'static str, error: impl std::fmt::Display) -> RecoveryError {
    RecoveryError::Storage {
        operation,
        reason: error.to_string(),
    }
}

fn recovery_to_intent(error: RecoveryError) -> IntentStoreError {
    IntentStoreError::Storage {
        operation: "access recovery storage",
        reason: error.to_string(),
    }
}

fn poisoned_recovery_lock() -> RecoveryError {
    RecoveryError::Storage {
        operation: "lock journal",
        reason: "recovery operation lock is poisoned".into(),
    }
}

fn poisoned_intent_lock() -> IntentStoreError {
    IntentStoreError::Storage {
        operation: "lock checkpoint intents",
        reason: "recovery operation lock is poisoned".into(),
    }
}
