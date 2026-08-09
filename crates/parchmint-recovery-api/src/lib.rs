//! Durable, editor-independent recovery journal contracts.
//!
//! A recovery implementation persists [`RecoveryBatch`] values in append order.
//! This crate owns the validation and replay rules so every implementation makes
//! the same conservative decision when a journal cannot be interpreted safely.

use std::{collections::BTreeMap, error::Error, fmt};

pub use parchmint_domain::{DocumentId, ProjectRevision};
pub use parchmint_project_format::{ContentHash, ResourceId};

use parchmint_contracts::generated::RecoveryRecordV1;
use sha2::{Digest, Sha256};

/// A monotonic per-document revision, independent of any editor engine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentRevision(u64);

impl DocumentRevision {
    pub const fn value(self) -> u64 {
        self.0
    }

    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl From<u64> for DocumentRevision {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// The inclusive document revisions covered by one recovery batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorRevisionRange {
    pub first: DocumentRevision,
    pub last: DocumentRevision,
}

impl EditorRevisionRange {
    pub fn new(first: DocumentRevision, last: DocumentRevision) -> Result<Self, RecoveryError> {
        if first > last {
            return Err(RecoveryError::InvalidBatch {
                field: "document revision range",
                reason: "first revision must not follow last revision",
            });
        }
        Ok(Self { first, last })
    }
}

/// A complete revision frontier for recovery work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryRevisionVector {
    pub project_revision: ProjectRevision,
    pub documents: BTreeMap<DocumentId, DocumentRevision>,
}

impl RecoveryRevisionVector {
    pub fn new(
        project_revision: ProjectRevision,
        documents: BTreeMap<DocumentId, DocumentRevision>,
    ) -> Self {
        Self {
            project_revision,
            documents,
        }
    }
}

/// Exact revisions already represented by a completed canonical save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableRevisionVector {
    pub revisions: RecoveryRevisionVector,
}

impl DurableRevisionVector {
    pub fn new(revisions: RecoveryRevisionVector) -> Self {
        Self { revisions }
    }
}

/// The last fully saved canonical state from which replay starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryBaseSnapshot {
    pub revisions: RecoveryRevisionVector,
    pub hashes: BTreeMap<ResourceId, ContentHash>,
}

/// A supported, versioned recovery payload.
///
/// The payload contains contract-level operations, never editor transactions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionedRecoveryPayload {
    V1(RecoveryRecordV1),
}

impl VersionedRecoveryPayload {
    pub fn validate(&self) -> Result<(), RecoveryError> {
        match self {
            Self::V1(record) => {
                if record.schema != "parchmint.recovery-record/v1" {
                    return Err(RecoveryError::UnsupportedPayloadVersion {
                        version: record.schema.clone(),
                    });
                }
                if record.record_id.trim().is_empty() {
                    return Err(RecoveryError::InvalidBatch {
                        field: "payload record ID",
                        reason: "must not be empty",
                    });
                }
                if record
                    .operations
                    .iter()
                    .any(|operation| !operation.is_object())
                {
                    return Err(RecoveryError::InvalidBatch {
                        field: "payload operations",
                        reason: "must contain JSON objects",
                    });
                }
                Ok(())
            }
        }
    }
}

/// One ordered, revisioned group of recoverable canonical edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryBatch {
    pub project_revision: ProjectRevision,
    pub documents: BTreeMap<DocumentId, EditorRevisionRange>,
    pub base_hashes: BTreeMap<ResourceId, ContentHash>,
    pub result_hashes: BTreeMap<ResourceId, ContentHash>,
    pub payload: VersionedRecoveryPayload,
}

impl RecoveryBatch {
    /// Checks invariants that do not depend on the preceding journal record.
    pub fn validate(&self) -> Result<(), RecoveryError> {
        for range in self.documents.values() {
            EditorRevisionRange::new(range.first, range.last)?;
        }
        if !self.base_hashes.keys().eq(self.result_hashes.keys()) {
            return Err(RecoveryError::InvalidBatch {
                field: "resource hashes",
                reason: "base and result hashes must cover the same resources",
            });
        }
        if self.base_hashes.is_empty()
            || self
                .base_hashes
                .iter()
                .all(|(resource, base)| self.result_hashes[resource] == *base)
        {
            return Err(RecoveryError::InvalidBatch {
                field: "resource hashes",
                reason: "must describe at least one changed resource",
            });
        }
        self.payload.validate()
    }

    /// The exact revision frontier reached when this batch is accepted.
    pub fn revision_vector(&self) -> RecoveryRevisionVector {
        RecoveryRevisionVector {
            project_revision: self.project_revision,
            documents: self
                .documents
                .iter()
                .map(|(document, range)| (*document, range.last))
                .collect(),
        }
    }

    /// Checks append order against the immediately preceding valid batch.
    pub fn validate_after(&self, previous: Option<&RecoveryBatch>) -> Result<(), RecoveryError> {
        self.validate()?;
        let Some(previous) = previous else {
            return Ok(());
        };
        let expected = previous.project_revision.next();
        if self.project_revision != expected {
            return Err(RecoveryError::NonConsecutiveProjectRevision {
                expected,
                actual: self.project_revision,
            });
        }
        for (document, range) in &self.documents {
            let expected = previous
                .documents
                .get(document)
                .map_or_else(|| DocumentRevision::from(1), |range| range.last.next());
            if range.first != expected {
                return Err(RecoveryError::NonConsecutiveDocumentRevision {
                    document: *document,
                    expected,
                    actual: range.first,
                });
            }
        }
        for (resource, expected) in &previous.result_hashes {
            if let Some(actual) = self.base_hashes.get(resource)
                && actual != expected
            {
                return Err(RecoveryError::HashMismatch {
                    resource: resource.clone(),
                    expected: *expected,
                    actual: *actual,
                });
            }
        }
        Ok(())
    }
}

/// A record loaded from a durable journal.
///
/// Implementations retain records they cannot safely decode so callers can
/// inspect or export them without accidentally applying them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryRecord {
    Complete(RecoveryBatch),
    UnknownVersion {
        project_revision: Option<ProjectRevision>,
        version: String,
    },
    Truncated {
        project_revision: Option<ProjectRevision>,
    },
    Mismatched {
        project_revision: Option<ProjectRevision>,
        reason: String,
    },
    Ambiguous {
        project_revision: Option<ProjectRevision>,
    },
}

impl RecoveryRecord {
    fn project_revision(&self) -> Option<ProjectRevision> {
        match self {
            Self::Complete(batch) => Some(batch.project_revision),
            Self::UnknownVersion {
                project_revision, ..
            }
            | Self::Truncated { project_revision }
            | Self::Mismatched {
                project_revision, ..
            }
            | Self::Ambiguous { project_revision } => *project_revision,
        }
    }
}

/// Opaque identity binding a durable receipt to one exact recovery batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecoveryReceiptId([u8; 32]);

/// A receipt for recovery data known to have reached durable storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryReceipt {
    pub durable_through: RecoveryRevisionVector,
    receipt_id: RecoveryReceiptId,
}

impl RecoveryReceipt {
    /// Creates the only receipt identity accepted for this exact batch.
    pub fn for_batch(batch: &RecoveryBatch) -> Self {
        let mut digest = Sha256::new();
        digest.update(batch.project_revision.value().to_le_bytes());
        for (document, range) in &batch.documents {
            digest.update(document.as_bytes());
            digest.update(range.first.value().to_le_bytes());
            digest.update(range.last.value().to_le_bytes());
        }
        for (resource, hash) in &batch.base_hashes {
            digest.update(format!("{resource:?}").as_bytes());
            digest.update(hash.as_bytes());
        }
        for (resource, hash) in &batch.result_hashes {
            digest.update(format!("{resource:?}").as_bytes());
            digest.update(hash.as_bytes());
        }
        digest.update(format!("{:?}", batch.payload).as_bytes());
        Self {
            durable_through: batch.revision_vector(),
            receipt_id: RecoveryReceiptId(digest.finalize().into()),
        }
    }

    pub fn receipt_id(&self) -> RecoveryReceiptId {
        self.receipt_id
    }

    pub fn authenticates(&self, batch: &RecoveryBatch) -> bool {
        self == &Self::for_batch(batch)
    }
}

/// A journal entry visible to recovery diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryRecordSummary {
    pub position: usize,
    pub project_revision: Option<ProjectRevision>,
}

/// The journal's current durable frontier and retained records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryInventory {
    pub records: Vec<RecoveryRecordSummary>,
    pub durable_through: Option<RecoveryRevisionVector>,
}

/// Why replay quarantined one record and every record after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryIsolation {
    pub position: usize,
    pub reason: RecoveryIsolationReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryIsolationReason {
    UnknownVersion { version: String },
    Truncated,
    Mismatched { reason: String },
    Ambiguous,
    InvalidBatch(RecoveryError),
}

/// The only records safe to offer to the save coordinator after a replay pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryReplay {
    pub accepted: Vec<RecoveryBatch>,
    pub isolated: Vec<RecoveryRecord>,
    pub isolation: Option<RecoveryIsolation>,
}

/// Checks ordered records against a saved snapshot without applying them.
///
/// Records at or before the saved project revision are already represented by
/// the base snapshot. The first unsafe later record and every following record
/// are returned in `isolated`; replay never attempts to skip over a defect.
pub fn replay_records(
    base: &RecoveryBaseSnapshot,
    records: impl IntoIterator<Item = RecoveryRecord>,
) -> RecoveryReplay {
    let records = records.into_iter().collect::<Vec<_>>();
    let mut expected_project = base.revisions.project_revision.next();
    let mut expected_documents = base.revisions.documents.clone();
    let mut expected_hashes = base.hashes.clone();
    let mut accepted = Vec::new();

    for (position, record) in records.iter().enumerate() {
        if record
            .project_revision()
            .is_some_and(|revision| revision <= base.revisions.project_revision)
        {
            continue;
        }

        let result = match record {
            RecoveryRecord::Complete(batch) => validate_replay_batch(
                batch,
                expected_project,
                &expected_documents,
                &expected_hashes,
            ),
            RecoveryRecord::UnknownVersion { version, .. } => {
                Err(RecoveryIsolationReason::UnknownVersion {
                    version: version.clone(),
                })
            }
            RecoveryRecord::Truncated { .. } => Err(RecoveryIsolationReason::Truncated),
            RecoveryRecord::Mismatched { reason, .. } => Err(RecoveryIsolationReason::Mismatched {
                reason: reason.clone(),
            }),
            RecoveryRecord::Ambiguous { .. } => Err(RecoveryIsolationReason::Ambiguous),
        };

        match result {
            Ok(()) => {
                let RecoveryRecord::Complete(batch) = record else {
                    unreachable!("only complete records can validate");
                };
                expected_project = batch.project_revision.next();
                for (document, range) in &batch.documents {
                    expected_documents.insert(*document, range.last);
                }
                for (resource, hash) in &batch.result_hashes {
                    expected_hashes.insert(resource.clone(), *hash);
                }
                accepted.push(batch.clone());
            }
            Err(reason) => {
                return RecoveryReplay {
                    accepted,
                    isolated: records[position..].to_vec(),
                    isolation: Some(RecoveryIsolation { position, reason }),
                };
            }
        }
    }

    RecoveryReplay {
        accepted,
        isolated: Vec::new(),
        isolation: None,
    }
}

/// Returns whether a retained journal contains exactly the requested frontier.
///
/// Storage implementations use this before issuing a durable flush receipt.
pub fn contains_revision_vector(
    records: &[RecoveryRecord],
    target: &RecoveryRevisionVector,
) -> bool {
    records.iter().any(|record| {
        matches!(record, RecoveryRecord::Complete(batch) if batch.revision_vector() == *target)
    })
}

/// Returns whether a completed save fully supersedes one valid recovery record.
///
/// Invalid records are deliberately never discardable: they remain available
/// for recovery diagnostics and manual review.
pub fn is_covered_by_durable(record: &RecoveryRecord, durable: &DurableRevisionVector) -> bool {
    let RecoveryRecord::Complete(batch) = record else {
        return false;
    };
    batch.project_revision <= durable.revisions.project_revision
        && batch.documents.iter().all(|(document, range)| {
            durable
                .revisions
                .documents
                .get(document)
                .is_some_and(|revision| range.last <= *revision)
        })
}

fn validate_replay_batch(
    batch: &RecoveryBatch,
    expected_project: ProjectRevision,
    expected_documents: &BTreeMap<DocumentId, DocumentRevision>,
    expected_hashes: &BTreeMap<ResourceId, ContentHash>,
) -> Result<(), RecoveryIsolationReason> {
    batch
        .validate()
        .map_err(RecoveryIsolationReason::InvalidBatch)?;
    if batch.project_revision != expected_project {
        return Err(RecoveryIsolationReason::InvalidBatch(
            RecoveryError::NonConsecutiveProjectRevision {
                expected: expected_project,
                actual: batch.project_revision,
            },
        ));
    }
    for (document, range) in &batch.documents {
        let expected = expected_documents
            .get(document)
            .copied()
            .unwrap_or_default()
            .next();
        if range.first != expected {
            return Err(RecoveryIsolationReason::InvalidBatch(
                RecoveryError::NonConsecutiveDocumentRevision {
                    document: *document,
                    expected,
                    actual: range.first,
                },
            ));
        }
    }
    for (resource, base_hash) in &batch.base_hashes {
        let Some(expected) = expected_hashes.get(resource) else {
            return Err(RecoveryIsolationReason::InvalidBatch(
                RecoveryError::MissingBaseHash {
                    resource: resource.clone(),
                },
            ));
        };
        if base_hash != expected {
            return Err(RecoveryIsolationReason::InvalidBatch(
                RecoveryError::HashMismatch {
                    resource: resource.clone(),
                    expected: *expected,
                    actual: *base_hash,
                },
            ));
        }
    }
    Ok(())
}

/// The report from reclaiming records that a completed save supersedes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionReport {
    pub removed_records: usize,
    pub retained_records: usize,
}

/// The report from deliberately discarding recovery records through a save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscardReport {
    pub removed_records: usize,
    pub retained_records: usize,
}

/// Recovery storage failures and validation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryError {
    InvalidBatch {
        field: &'static str,
        reason: &'static str,
    },
    UnsupportedPayloadVersion {
        version: String,
    },
    NonConsecutiveProjectRevision {
        expected: ProjectRevision,
        actual: ProjectRevision,
    },
    NonConsecutiveDocumentRevision {
        document: DocumentId,
        expected: DocumentRevision,
        actual: DocumentRevision,
    },
    MissingBaseHash {
        resource: ResourceId,
    },
    HashMismatch {
        resource: ResourceId,
        expected: ContentHash,
        actual: ContentHash,
    },
    UnknownRevisionVector,
    Storage {
        operation: &'static str,
        reason: String,
    },
}

impl fmt::Display for RecoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBatch { field, reason } => {
                write!(formatter, "invalid recovery {field}: {reason}")
            }
            Self::UnsupportedPayloadVersion { version } => {
                write!(formatter, "unsupported recovery payload version {version}")
            }
            Self::NonConsecutiveProjectRevision { expected, actual } => write!(
                formatter,
                "expected recovery project revision {}, got {}",
                expected.value(),
                actual.value()
            ),
            Self::NonConsecutiveDocumentRevision {
                document,
                expected,
                actual,
            } => write!(
                formatter,
                "expected recovery document {document:?} revision {}, got {}",
                expected.value(),
                actual.value()
            ),
            Self::MissingBaseHash { resource } => {
                write!(
                    formatter,
                    "recovery base snapshot has no hash for {resource:?}"
                )
            }
            Self::HashMismatch { resource, .. } => {
                write!(formatter, "recovery hash mismatch for {resource:?}")
            }
            Self::UnknownRevisionVector => formatter.write_str("unknown recovery revision vector"),
            Self::Storage { operation, reason } => {
                write!(formatter, "recovery {operation} failed: {reason}")
            }
        }
    }
}

impl Error for RecoveryError {}

/// Durable storage for append-only recovery records.
///
/// `append` preserves record order. `flush_through` returns only after the
/// requested exact revision vector is durable. `compact` and
/// `discard_through` may remove only records fully covered by the supplied
/// completed-save vector.
pub trait RecoveryJournal: Send + Sync {
    fn append(&self, batch: RecoveryBatch) -> Result<RecoveryReceipt, RecoveryError>;
    fn flush_through(
        &self,
        target: RecoveryRevisionVector,
    ) -> Result<RecoveryReceipt, RecoveryError>;
    fn inspect(&self) -> Result<RecoveryInventory, RecoveryError>;
    fn replay(&self, base: RecoveryBaseSnapshot) -> Result<RecoveryReplay, RecoveryError>;
    fn compact(&self, durable: DurableRevisionVector) -> Result<CompactionReport, RecoveryError>;
    fn discard_through(
        &self,
        durable: DurableRevisionVector,
    ) -> Result<DiscardReport, RecoveryError>;
}

#[cfg(test)]
mod recovery_journal_contract_tests;
