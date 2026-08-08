//! Rebuildable, whole-project search contracts.
//!
//! Search data is a disposable projection of canonical project state. Results
//! are revisioned candidates, not authority to navigate or replace text: the
//! caller must revalidate them against the current projection.

use std::{collections::BTreeSet, error::Error, fmt};

pub use parchmint_domain::{BlockId, DocumentId, MetadataFieldId, ProjectId};

/// A monotonic revision of one searchable document projection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RevisionId(u64);

impl RevisionId {
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl From<u64> for RevisionId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// The byte offsets of UTF-8 text in one indexed field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextRange {
    start: usize,
    end: usize,
}

impl TextRange {
    pub const fn new(start: usize, end: usize) -> Option<Self> {
        if start <= end {
            Some(Self { start, end })
        } else {
            None
        }
    }

    pub const fn start(self) -> usize {
        self.start
    }

    pub const fn end(self) -> usize {
        self.end
    }

    pub const fn len(self) -> usize {
        self.end - self.start
    }

    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Returns the range only when it selects valid UTF-8 boundaries in `text`.
    pub fn text(self, text: &str) -> Option<&str> {
        text.get(self.start..self.end)
    }
}

/// A searchable project field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SearchField {
    Body,
    DisplayTitle,
    Synopsis,
    Metadata(MetadataFieldId),
}

/// One revisioned, searchable text unit from a document projection.
///
/// `block_id` identifies the source unit used to navigate a hit. Providers
/// choose stable IDs for non-body fields as well as authored body blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchTextProjection {
    pub block_id: BlockId,
    pub field: SearchField,
    pub text: String,
}

/// All searchable text copied from one document at one revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchDocumentProjection {
    pub document_id: DocumentId,
    pub revision: RevisionId,
    pub texts: Vec<SearchTextProjection>,
}

impl SearchDocumentProjection {
    /// Returns the text for one exact source unit.
    pub fn text(&self, block_id: BlockId, field: SearchField) -> Option<&str> {
        self.texts
            .iter()
            .find(|text| text.block_id == block_id && text.field == field)
            .map(|text| text.text.as_str())
    }

    /// Rejects duplicate source units, which would make revalidation ambiguous.
    pub fn validate(&self) -> Result<(), SearchError> {
        let mut units = BTreeSet::new();
        for text in &self.texts {
            if !units.insert((text.block_id, text.field)) {
                return Err(SearchError::InvalidInput {
                    field: "search projection",
                    reason: "contains a duplicate block and field",
                });
            }
        }
        Ok(())
    }
}

/// Typed search options. The API never accepts SQL or index query syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub text: String,
    pub fields: BTreeSet<SearchField>,
    pub case_sensitive: bool,
    pub whole_word: bool,
    /// A UI-assigned request number. Batches for older numbers are stale.
    pub generation: u64,
}

impl SearchQuery {
    pub fn validate(&self) -> Result<(), SearchError> {
        if self.text.is_empty() {
            return Err(SearchError::InvalidInput {
                field: "search text",
                reason: "must not be empty",
            });
        }
        if self.fields.is_empty() {
            return Err(SearchError::InvalidInput {
                field: "search fields",
                reason: "must not be empty",
            });
        }
        Ok(())
    }
}

/// Context around a hit, with the precise matching bytes highlighted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSnippet {
    pub text: String,
    pub match_range: TextRange,
}

impl SearchSnippet {
    pub fn matched_text(&self) -> Option<&str> {
        self.match_range.text(&self.text)
    }
}

/// One possible match from an indexed projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub document_id: DocumentId,
    pub block_id: BlockId,
    pub indexed_revision: RevisionId,
    pub field: SearchField,
    pub candidate_range: TextRange,
    pub snippet: SearchSnippet,
}

/// A replacement candidate derived from a body-search hit.
///
/// It records both the indexed revision and exact matched text. Before the
/// application changes a document, [`Self::revalidates`] must succeed against
/// the current canonical or open-editor projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementCandidate {
    pub document_id: DocumentId,
    pub block_id: BlockId,
    pub indexed_revision: RevisionId,
    pub candidate_range: TextRange,
    expected_text: String,
}

impl ReplacementCandidate {
    pub fn from_hit(hit: &SearchHit) -> Result<Self, SearchError> {
        if hit.field != SearchField::Body {
            return Err(SearchError::InvalidInput {
                field: "replacement candidate",
                reason: "only document-body hits are replaceable",
            });
        }
        let expected_text = hit
            .snippet
            .matched_text()
            .filter(|text| !text.is_empty())
            .ok_or(SearchError::InvalidInput {
                field: "search hit snippet",
                reason: "must contain a non-empty matched range",
            })?
            .to_owned();
        Ok(Self {
            document_id: hit.document_id,
            block_id: hit.block_id,
            indexed_revision: hit.indexed_revision,
            candidate_range: hit.candidate_range,
            expected_text,
        })
    }

    pub fn expected_text(&self) -> &str {
        &self.expected_text
    }

    /// Checks identity, revision, field text, range boundaries, and exact bytes.
    pub fn revalidates(&self, current: &SearchDocumentProjection) -> bool {
        current.document_id == self.document_id
            && current.revision == self.indexed_revision
            && current
                .text(self.block_id, SearchField::Body)
                .and_then(|text| self.candidate_range.text(text))
                .is_some_and(|text| text == self.expected_text)
    }
}

/// One bounded delivery from a running search query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchBatch {
    /// The UI-assigned request number from [`SearchQuery`].
    pub generation: u64,
    pub hits: Vec<SearchHit>,
    /// True only for the final batch of a completed, non-cancelled query.
    pub finished: bool,
}

/// Receives bounded batches from a background search operation.
pub trait SearchBatchSink: Send + Sync {
    fn push(&self, batch: SearchBatch);
}

/// The state reached when the index opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchIndexState {
    Opened,
    Rebuilt { previous: SearchIndexProblem },
}

/// A disposable-index condition that requires rebuilding from canonical data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchIndexProblem {
    Missing,
    Corrupt,
    Incompatible,
}

/// The result of a non-mutating integrity check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchIntegrityReport {
    pub indexed_documents: usize,
    pub healthy: bool,
    pub problem: Option<SearchIndexProblem>,
}

/// The work completed while replacing an invalid index from its source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebuildReport {
    pub indexed_documents: usize,
}

/// A receipt for a revisioned index update.
///
/// `replaced` is false when the index already holds a newer projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionReceipt {
    pub document_id: DocumentId,
    pub indexed_revision: RevisionId,
    pub replaced: bool,
}

/// Visits canonical or open-editor projections without granting write access.
pub trait SearchProjectionVisitor {
    fn visit(&mut self, projection: SearchDocumentProjection) -> Result<(), SearchError>;
}

/// Supplies canonical search projections for a rebuild.
///
/// Implementations stream projections to `visitor`; they must not need to load
/// every project document at once and must never mutate authored data.
pub trait SearchProjectionSource: Send + Sync {
    fn visit_projections(
        &self,
        visitor: &mut dyn SearchProjectionVisitor,
    ) -> Result<(), SearchError>;
}

/// Search failures leave canonical project files and editor state untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchError {
    InvalidInput {
        field: &'static str,
        reason: &'static str,
    },
    Storage {
        operation: &'static str,
        reason: String,
    },
    Source {
        reason: String,
    },
}

impl fmt::Display for SearchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { field, reason } => {
                write!(formatter, "invalid search {field}: {reason}")
            }
            Self::Storage { operation, reason } => {
                write!(formatter, "search index {operation} failed: {reason}")
            }
            Self::Source { reason } => {
                write!(formatter, "search projection source failed: {reason}")
            }
        }
    }
}

impl Error for SearchError {}

/// Rebuildable storage for revisioned whole-project search projections.
///
/// Implementations replace or delete one document atomically with its indexed
/// revision. `open_or_rebuild` verifies the disposable cache and rebuilds a
/// missing, corrupt, or incompatible cache from `source` without changing
/// authored data. Query work streams bounded batches to `sink`. The UI assigns
/// each query generation and ignores batches for older generations. After
/// `cancel` returns, the index must not push another batch for that generation.
/// A hit remains a candidate until the caller revalidates it before acting.
pub trait SearchIndex: Send + Sync {
    fn open_or_rebuild(
        &self,
        project: ProjectId,
        source: &dyn SearchProjectionSource,
    ) -> Result<SearchIndexState, SearchError>;
    fn replace_document(
        &self,
        projection: SearchDocumentProjection,
    ) -> Result<ProjectionReceipt, SearchError>;
    fn delete_document(
        &self,
        id: DocumentId,
        revision: RevisionId,
    ) -> Result<ProjectionReceipt, SearchError>;
    fn query(&self, query: SearchQuery, sink: Box<dyn SearchBatchSink>) -> Result<(), SearchError>;
    fn cancel(&self, generation: u64);
    fn verify(&self) -> Result<SearchIntegrityReport, SearchError>;
    fn rebuild(&self, source: &dyn SearchProjectionSource) -> Result<RebuildReport, SearchError>;
}

#[cfg(test)]
mod search_index_contract_tests;
