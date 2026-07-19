//! Qt-free domain primitives shared by application services and boundary DTOs.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::num::NonZeroU64;
use thiserror::Error;

/// Identifies one open-project incarnation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ProjectGeneration(NonZeroU64);

impl ProjectGeneration {
    /// Creates a generation, rejecting zero because it represents no project.
    pub fn new(value: u64) -> Result<Self, RevisionError> {
        NonZeroU64::new(value).map(Self).ok_or(RevisionError::Zero)
    }

    /// Returns the wire representation.
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

/// Monotonic revision of one document or derived resource.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct Revision(u64);

impl Revision {
    /// The first revision of a loaded resource.
    pub const INITIAL: Self = Self(0);

    /// Creates a revision from its wire value.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the wire representation.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Advances the revision, failing instead of wrapping.
    pub fn next(self) -> Result<Self, RevisionError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(RevisionError::Overflow)
    }
}

/// Correlates asynchronous work with the state that requested it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct WorkStamp {
    /// Open-project incarnation.
    pub generation: ProjectGeneration,
    /// Resource revision at submission time.
    pub revision: Revision,
}

impl WorkStamp {
    /// Returns true only when this result still targets current state.
    pub fn is_current(self, generation: ProjectGeneration, revision: Revision) -> bool {
        self.generation == generation && self.revision == revision
    }
}

/// Stable editor-boundary representation; it intentionally contains no Qt types.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EditorSnapshot {
    /// Stable document identifier serialized at the bridge boundary.
    pub document_id: String,
    /// Revision represented by this snapshot.
    pub revision: Revision,
    /// Semantic blocks in deterministic document order.
    pub blocks: Vec<EditorBlock>,
}

/// Minimal semantic block contract proven during the Stage 01 spike.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EditorBlock {
    /// A paragraph with a stable paragraph style key.
    Paragraph {
        /// Stable paragraph style identifier.
        style_id: String,
        /// Plain semantic text for the initial boundary spike.
        text: String,
    },
    /// A semantic page-break compile marker.
    PageBreak,
    /// Source-backed content unsupported by the current visual editor.
    Opaque {
        /// Exact source retained for later codec support or raw editing.
        source: String,
        /// User-displayable reason visual editing is unavailable.
        reason: String,
    },
}

/// Errors produced by monotonic identifiers.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum RevisionError {
    /// Zero cannot identify an open project.
    #[error("project generation must be non-zero")]
    Zero,
    /// The revision space has been exhausted.
    #[error("revision counter overflowed")]
    Overflow,
}

impl fmt::Display for WorkStamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}",
            self.generation.get(),
            self.revision.get()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_result_is_rejected_by_generation_or_revision() {
        let one = ProjectGeneration::new(1).unwrap();
        let two = ProjectGeneration::new(2).unwrap();
        let stamp = WorkStamp {
            generation: one,
            revision: Revision::new(4),
        };
        assert!(stamp.is_current(one, Revision::new(4)));
        assert!(!stamp.is_current(two, Revision::new(4)));
        assert!(!stamp.is_current(one, Revision::new(5)));
    }
}
