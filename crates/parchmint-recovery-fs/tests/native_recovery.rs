use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use parchmint_domain::{CheckpointId, ProjectId, ProjectRevision};
use parchmint_history_api::{CheckpointCategory, CheckpointInput, CheckpointIntentHash};
use parchmint_project_format::{CanonicalRelativePath, ContentHash, ResourceId};
use parchmint_project_repository::{AtomicWritePlan, CommitReceipt, StagedResource};
use parchmint_recovery_api::{
    DurableRevisionVector, EditorRevisionRange, RecoveryBaseSnapshot, RecoveryBatch,
    RecoveryJournal as _, RecoveryRecord, RecoveryRevisionVector, VersionedRecoveryPayload,
};
use parchmint_save::{
    CheckpointIntent, CheckpointIntentState, CheckpointIntentStore as _, CheckpointReceipt,
    SavePriority, SaveRevisionVector,
};
use serde_json::json;

struct TestDir(PathBuf);

impl TestDir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("parchmint-recovery-{name}-{}", std::process::id()));
        fs::create_dir_all(&path).expect("test directory should be creatable");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn hash(value: u8) -> ContentHash {
    ContentHash::from_bytes([value; 32])
}

fn batch(revision: u64, base: u8, result: u8) -> RecoveryBatch {
    RecoveryBatch {
        project_revision: ProjectRevision::from(revision),
        documents: BTreeMap::from([(
            parchmint_recovery_api::DocumentId::from_bytes([7; 16]),
            EditorRevisionRange::new(revision.into(), revision.into()).expect("valid range"),
        )]),
        base_hashes: BTreeMap::from([(ResourceId::Manifest, hash(base))]),
        result_hashes: BTreeMap::from([(ResourceId::Manifest, hash(result))]),
        payload: VersionedRecoveryPayload::V1(parchmint_contracts::generated::RecoveryRecordV1 {
            schema: "parchmint.recovery-record/v1".into(),
            record_id: format!("recovery-{revision}"),
            operations: vec![json!({"replace": "manifest", "revision": revision})],
        }),
    }
}

fn vector(revision: u64) -> RecoveryRevisionVector {
    RecoveryRevisionVector::new(
        ProjectRevision::from(revision),
        BTreeMap::from([(
            parchmint_recovery_api::DocumentId::from_bytes([7; 16]),
            revision.into(),
        )]),
    )
}

fn journal_path(dir: &TestDir) -> PathBuf {
    dir.path()
        .join(".parchmint")
        .join("recovery")
        .join("journal.bin")
}

fn open(dir: &TestDir) -> parchmint_recovery_fs::FsRecoveryJournal {
    parchmint_recovery_fs::FsRecoveryJournal::open(dir.path()).expect("recovery root should open")
}

fn replay_from(
    journal: &parchmint_recovery_fs::FsRecoveryJournal,
    revision: u64,
    hash_value: u8,
) -> parchmint_recovery_api::RecoveryReplay {
    journal
        .replay(RecoveryBaseSnapshot {
            revisions: vector(revision),
            hashes: BTreeMap::from([(ResourceId::Manifest, hash(hash_value))]),
        })
        .expect("replay should succeed")
}

fn intent(hash_value: u8) -> CheckpointIntent {
    let path = CanonicalRelativePath::parse("project.toml").expect("canonical path");
    let revisions = SaveRevisionVector {
        project_revision: ProjectRevision::from(1),
        open_documents: BTreeMap::new(),
        closed_resources: BTreeMap::new(),
        canonical_hashes: BTreeMap::from([(ResourceId::Manifest, hash(1))]),
        generation: 1.into(),
    };
    CheckpointIntent {
        project: ProjectId::from_bytes([3; 16]),
        revisions,
        writes: AtomicWritePlan::new(vec![StagedResource {
            path: "project.toml".into(),
            bytes: b"draft".to_vec(),
        }]),
        checkpoint: CheckpointInput {
            intent_hash: CheckpointIntentHash::from_bytes([hash_value; 32]),
            resources: BTreeMap::from([(path, hash(1))]),
            category: CheckpointCategory::ExplicitSave,
            affected_documents: Vec::new(),
            name: None,
            recorded_at_unix_millis: Some(u64::from(hash_value)),
        },
        priority: SavePriority::Explicit,
        state: CheckpointIntentState::Planned,
    }
}

#[test]
fn framed_appends_return_exact_receipts_and_retries_do_not_duplicate() {
    let dir = TestDir::new("frames");
    let journal = open(&dir);
    let first = batch(1, 0, 1);

    let receipt = journal
        .append(first.clone())
        .expect("append should succeed");
    assert_eq!(receipt.durable_through, vector(1));
    let bytes = fs::read(journal_path(&dir)).expect("journal should exist");
    assert_eq!(&bytes[..4], b"PMRJ");
    assert!(bytes.len() > 56, "a frame must include a payload");

    let retry = journal.append(first).expect("append retry should succeed");
    assert_eq!(retry.durable_through, vector(1));
    assert_eq!(
        journal
            .inspect()
            .expect("inspect after retry should succeed")
            .records
            .len(),
        1
    );
    journal.append(batch(2, 1, 2)).expect("second append");

    let receipt = journal
        .flush_through(vector(1))
        .expect("flush should succeed");
    assert_eq!(receipt.durable_through, vector(1));
}

#[test]
fn invalid_suffixes_quarantine_only_the_valid_prefix() {
    let dir = TestDir::new("truncated");
    let journal = open(&dir);
    journal.append(batch(1, 0, 1)).expect("append");
    journal.append(batch(2, 1, 2)).expect("append");
    let path = journal_path(&dir);
    let mut bytes = fs::read(&path).expect("journal");
    bytes.truncate(bytes.len() - 3);
    fs::write(path, bytes).expect("truncate tail");

    let reopened = open(&dir);
    let replay = replay_from(&reopened, 0, 0);
    assert_eq!(replay.accepted.len(), 1);
    assert!(matches!(
        replay.isolated.first(),
        Some(RecoveryRecord::Truncated { .. })
    ));

    let dir = TestDir::new("checksum");
    let journal = open(&dir);
    journal.append(batch(1, 0, 1)).expect("append");
    journal.append(batch(2, 1, 2)).expect("append");
    journal.append(batch(3, 2, 3)).expect("append");
    let path = journal_path(&dir);
    let mut bytes = fs::read(&path).expect("journal");
    let first_len =
        56 + u64::from_le_bytes(bytes[8..16].try_into().expect("frame length")) as usize;
    let second_len = 56
        + u64::from_le_bytes(
            bytes[first_len + 8..first_len + 16]
                .try_into()
                .expect("frame length"),
        ) as usize;
    bytes[first_len + second_len - 1] ^= 0xff;
    fs::write(path, bytes).expect("corrupt checksum");

    let replay = replay_from(&open(&dir), 0, 0);
    assert_eq!(replay.accepted, vec![batch(1, 0, 1)]);
    assert_eq!(
        replay.isolated.len(),
        2,
        "the corrupt record and later records are quarantined"
    );
    assert!(matches!(
        replay.isolated.first(),
        Some(RecoveryRecord::Mismatched { .. })
    ));
}

#[test]
fn checkpoint_intents_persist_require_commit_and_complete_idempotently() {
    let dir = TestDir::new("intents");
    let journal = open(&dir);
    let pending = intent(9);
    journal
        .persist(pending.clone())
        .expect("intent should persist");
    assert_eq!(
        open(&dir).pending().expect("pending intents after reopen"),
        vec![pending.clone()]
    );
    let receipt = CheckpointReceipt {
        project: pending.project,
        intent_hash: pending.intent_hash(),
        checkpoint: CheckpointId::from_bytes([8; 16]),
        revisions: pending.revisions.clone(),
    };
    assert!(journal.complete(receipt.clone()).is_err());
    assert_eq!(journal.pending().expect("planned intent remains").len(), 1);

    let mut committed = pending.clone();
    committed.state = CheckpointIntentState::FilesCommitted {
        receipt: CommitReceipt::new(17),
    };
    journal
        .persist(committed.clone())
        .expect("committed file state should persist");
    assert_eq!(
        open(&dir).pending().expect("committed intent after reopen"),
        vec![committed]
    );
    journal
        .complete(receipt.clone())
        .expect("completion should succeed");
    open(&dir)
        .complete(receipt)
        .expect("completion after reopen should be a no-op");
    assert!(journal.pending().expect("pending intents").is_empty());
}

#[test]
fn compaction_removes_only_saved_records_and_retains_newer_edits() {
    let dir = TestDir::new("compaction");
    let journal = open(&dir);
    journal.append(batch(1, 0, 1)).expect("append");
    journal.append(batch(2, 1, 2)).expect("append");
    journal.append(batch(3, 2, 3)).expect("append");

    let report = journal
        .compact(DurableRevisionVector::new(vector(1)))
        .expect("compact");
    assert_eq!((report.removed_records, report.retained_records), (1, 2));
    let replay = replay_from(&journal, 1, 1);
    assert_eq!(
        replay
            .accepted
            .iter()
            .map(|record| record.project_revision.value())
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
}

#[test]
fn recovery_paths_cannot_escape_the_recovery_directory() {
    let dir = TestDir::new("paths");
    assert!(
        parchmint_recovery_fs::FsRecoveryJournal::open(
            dir.path().join(".parchmint/recovery/../outside")
        )
        .is_err()
    );

    #[cfg(unix)]
    {
        let journal = open(&dir);
        let path = journal_path(&dir);
        fs::remove_file(&path).expect("journal fixture should be removable");
        std::os::unix::fs::symlink(dir.path().join("outside"), &path).expect("symlink fixture");
        assert!(journal.append(batch(1, 0, 1)).is_err());
        assert!(
            !dir.path().join("outside").exists(),
            "writes must not follow the symlink"
        );
    }
}
