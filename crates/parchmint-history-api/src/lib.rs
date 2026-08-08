//! Git-independent contracts for complete ParchMint project History.
//!
//! A History implementation stores immutable checkpoints of the canonical
//! project resource set.  Restoring a checkpoint only returns a complete
//! write plan; the normal save path applies that plan and records a new
//! restoration checkpoint, so existing History is never rewound.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub use parchmint_domain::{CheckpointId, DocumentId};
pub use parchmint_project_format::{CanonicalRelativePath, ContentHash};
pub use parchmint_project_repository::{AtomicWritePlan, ProjectRootCapability, StagedResource};

/// A stable, opaque key for a completed save attempt.
///
/// Retrying the same intent with the same complete resource set must return
/// the checkpoint ID first returned for that intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CheckpointIntentHash([u8; 32]);

impl CheckpointIntentHash {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// The user-visible reason a checkpoint was recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointCategory {
    Autosave,
    ExplicitSave,
    StructuralChange,
    NamedSnapshot,
    Restoration,
}

/// A non-empty name supplied for a [`CheckpointCategory::NamedSnapshot`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnapshotName(String);

impl SnapshotName {
    pub fn new(name: impl Into<String>) -> Result<Self, HistoryError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(HistoryError::InvalidInput {
                field: "snapshot name",
                reason: "must not be empty",
            });
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The complete, already-written canonical project state to checkpoint.
///
/// `resources` is keyed by canonical relative path so documents and their
/// annotation sidecars remain individually identifiable.  It contains only
/// project resources; recovery, caches, workspace state, and global settings
/// are outside History.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointInput {
    pub intent_hash: CheckpointIntentHash,
    pub resources: BTreeMap<CanonicalRelativePath, ContentHash>,
    pub category: CheckpointCategory,
    pub affected_documents: Vec<DocumentId>,
    pub name: Option<SnapshotName>,
}

impl CheckpointInput {
    pub fn validate(&self) -> Result<(), HistoryError> {
        match (self.category, self.name.as_ref()) {
            (CheckpointCategory::NamedSnapshot, None) => Err(HistoryError::InvalidInput {
                field: "name",
                reason: "named snapshots require a name",
            }),
            (CheckpointCategory::NamedSnapshot, Some(_)) | (_, None) => Ok(()),
            (_, Some(_)) => Err(HistoryError::InvalidInput {
                field: "name",
                reason: "only named snapshots may have a name",
            }),
        }
    }
}

/// An opaque continuation token returned by [`HistoryStore::list`].
///
/// Callers must return it unchanged with the same query filter.  Implementors
/// reject cursors that do not belong to their History or query.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HistoryCursor(String);

impl HistoryCursor {
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One newest-first page request over the immutable History sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPageQuery {
    pub cursor: Option<HistoryCursor>,
    pub limit: usize,
    pub affected_document: Option<DocumentId>,
}

impl HistoryPageQuery {
    pub fn newest_first(limit: usize) -> Self {
        Self {
            cursor: None,
            limit,
            affected_document: None,
        }
    }

    pub fn validate(&self) -> Result<(), HistoryError> {
        if self.limit == 0 {
            return Err(HistoryError::InvalidInput {
                field: "page limit",
                reason: "must be greater than zero",
            });
        }
        Ok(())
    }
}

/// Metadata displayed while browsing History.
///
/// `sequence` increases as checkpoints are appended.  List pages always
/// return summaries in descending sequence order, including when filtered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointSummary {
    pub id: CheckpointId,
    pub sequence: u64,
    pub category: CheckpointCategory,
    pub affected_documents: Vec<DocumentId>,
    pub name: Option<SnapshotName>,
}

/// A stable newest-first page of checkpoint summaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPage {
    pub checkpoints: Vec<CheckpointSummary>,
    pub next_cursor: Option<HistoryCursor>,
}

/// The complete resource manifest for a checkpoint preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotPreview {
    pub checkpoint: CheckpointSummary,
    pub resources: BTreeMap<CanonicalRelativePath, ContentHash>,
}

/// All canonical files to write when restoring one checkpoint.
///
/// The plan is intentionally whole-project: it never represents a
/// document-only restore.  Applying it is the responsibility of the normal
/// save path, which records a later restoration checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePlan {
    source: CheckpointId,
    resources: BTreeMap<CanonicalRelativePath, ContentHash>,
    writes: AtomicWritePlan,
}

impl RestorePlan {
    pub fn new(
        source: CheckpointId,
        resources: BTreeMap<CanonicalRelativePath, ContentHash>,
        writes: AtomicWritePlan,
    ) -> Result<Self, HistoryError> {
        let write_paths: Result<BTreeSet<_>, _> = writes
            .writes
            .iter()
            .map(|write| {
                CanonicalRelativePath::parse(&write.path).map_err(|_| HistoryError::InvalidInput {
                    field: "restore write path",
                    reason: "must be a canonical project-relative path",
                })
            })
            .collect();
        let write_paths = write_paths?;
        if write_paths.len() != writes.writes.len() || !write_paths.iter().eq(resources.keys()) {
            return Err(HistoryError::InvalidInput {
                field: "restore writes",
                reason: "must contain exactly the checkpoint resource set",
            });
        }
        Ok(Self {
            source,
            resources,
            writes,
        })
    }

    pub const fn source(&self) -> CheckpointId {
        self.source
    }

    pub fn resources(&self) -> &BTreeMap<CanonicalRelativePath, ContentHash> {
        &self.resources
    }

    pub fn writes(&self) -> &AtomicWritePlan {
        &self.writes
    }
}

/// The successful result of opening or creating project History.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryState {
    pub project: ProjectRootCapability,
    pub checkpoint_count: usize,
}

/// The result of a non-mutating History integrity pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryIntegrityReport {
    pub checked_checkpoints: usize,
}

/// A bounded amount of low-priority History maintenance work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaintenanceBudget {
    pub max_objects: usize,
}

impl MaintenanceBudget {
    pub const fn new(max_objects: usize) -> Self {
        Self { max_objects }
    }
}

/// The retained History and work completed by one maintenance pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceReport {
    pub checked_objects: usize,
    pub retained_checkpoints: usize,
}

/// A History failure that leaves the current canonical project files alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryError {
    MissingHistory,
    CorruptHistory {
        reason: String,
    },
    UnknownCheckpoint {
        checkpoint: CheckpointId,
    },
    InvalidCursor,
    InvalidInput {
        field: &'static str,
        reason: &'static str,
    },
    Storage {
        operation: &'static str,
        reason: String,
    },
}

impl fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHistory => formatter.write_str("project History is missing"),
            Self::CorruptHistory { reason } => {
                write!(formatter, "project History is corrupt: {reason}")
            }
            Self::UnknownCheckpoint { checkpoint } => {
                write!(formatter, "unknown History checkpoint {checkpoint:?}")
            }
            Self::InvalidCursor => formatter.write_str("invalid History page cursor"),
            Self::InvalidInput { field, reason } => {
                write!(formatter, "invalid History {field}: {reason}")
            }
            Self::Storage { operation, reason } => {
                write!(formatter, "History {operation} failed: {reason}")
            }
        }
    }
}

impl Error for HistoryError {}

/// Storage for an append-only sequence of complete project checkpoints.
///
/// Implementations must validate every checkpoint's canonical resource set,
/// return the original ID for an identical checkpoint intent retry, and keep
/// errors isolated from current-project reads.  `restore` returns data only;
/// it cannot move, rewrite, or delete existing History.
pub trait HistoryStore: Send + Sync {
    fn initialize(&self, project: ProjectRootCapability) -> Result<HistoryState, HistoryError>;
    fn checkpoint(&self, input: CheckpointInput) -> Result<CheckpointId, HistoryError>;
    fn list(&self, query: HistoryPageQuery) -> Result<HistoryPage, HistoryError>;
    fn preview(&self, checkpoint: CheckpointId) -> Result<SnapshotPreview, HistoryError>;
    fn restore(&self, checkpoint: CheckpointId) -> Result<RestorePlan, HistoryError>;
    fn verify(&self) -> Result<HistoryIntegrityReport, HistoryError>;
    fn maintain(&self, budget: MaintenanceBudget) -> Result<MaintenanceReport, HistoryError>;
}

#[cfg(test)]
mod history_store_contract_tests;
