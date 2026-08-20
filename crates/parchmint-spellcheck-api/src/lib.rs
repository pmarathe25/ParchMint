//! Engine-neutral contracts for offline en-US spellcheck.
//!
//! The public values in this crate contain ParchMint revisions, ranges, words,
//! and rankings only. The private spelling runtime belongs behind this boundary.

pub use parchmint_domain::{BlockId, DocumentId, ProjectId};
pub use parchmint_editor_api::{AsyncResult, EditorRevision, EditorSelection, EventStream};

/// The spellcheck language selected for one request.
///
/// V1 deliberately exposes only the bundled, offline `en-US` dictionary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum LanguageId {
    EnUs,
}

impl LanguageId {
    /// The stable language tag used in saved preferences and diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EnUs => "en-US",
        }
    }
}

/// A ParchMint-assigned request number for one spellcheck generation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpellcheckGeneration(u64);

impl SpellcheckGeneration {
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl From<u64> for SpellcheckGeneration {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// A monotonic revision of either a project or global dictionary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DictionaryRevision(u64);

impl DictionaryRevision {
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl From<u64> for DictionaryRevision {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// A service-issued opaque identifier for a cancellable spellcheck operation.
///
/// This is a ParchMint value, not a spelling-engine task or operating-system
/// handle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpellcheckHandle(u64);

impl SpellcheckHandle {
    /// Creates a service-owned operation identifier.
    pub const fn new(operation: u64) -> Self {
        Self(operation)
    }
}

impl From<u64> for SpellcheckHandle {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

/// The urgency assigned to bounded spellcheck work.
///
/// Implementations must schedule visible work before recently changed and
/// background work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpellcheckPriority {
    Visible,
    RecentlyChanged,
    Background,
}

/// Text copied from one stable editor block at a known document revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionedTextRange {
    pub block_id: BlockId,
    pub range: EditorSelection,
    pub text: String,
}

/// Bounded text to check against the current offline dictionary revisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellcheckRequest {
    pub language: LanguageId,
    pub document_id: DocumentId,
    pub project_id: ProjectId,
    pub document_revision: EditorRevision,
    pub blocks: Vec<RevisionedTextRange>,
    pub project_dictionary: DictionaryRevision,
    pub global_dictionary: DictionaryRevision,
    pub generation: SpellcheckGeneration,
    pub priority: SpellcheckPriority,
}

/// One ranked replacement proposed for a misspelled word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellingSuggestion {
    pub word: String,
    pub rank: SuggestionRank,
}

/// A stable ordering rank assigned to one spelling suggestion.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SuggestionRank(u64);

impl SuggestionRank {
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl From<u64> for SuggestionRank {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// The spelling condition associated with an underlined word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpellingCategory {
    Misspelling,
}

/// One misspelled word located in a revisioned editor block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellingIssue {
    pub block_id: BlockId,
    pub range: EditorSelection,
    pub word: String,
    pub category: SpellingCategory,
    pub suggestions: Vec<SpellingSuggestion>,
}

/// One result batch produced for an exact spellcheck generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellcheckResult {
    pub document_id: DocumentId,
    pub document_revision: EditorRevision,
    pub project_dictionary: DictionaryRevision,
    pub global_dictionary: DictionaryRevision,
    pub generation: SpellcheckGeneration,
    pub issues: Vec<SpellingIssue>,
}

impl SpellcheckRequest {
    /// Returns true only when `result` was produced for this exact text and
    /// dictionary generation. Callers discard every other result as stale.
    pub fn accepts(&self, result: &SpellcheckResult) -> bool {
        self.document_id == result.document_id
            && self.document_revision == result.document_revision
            && self.project_dictionary == result.project_dictionary
            && self.global_dictionary == result.global_dictionary
            && self.generation == result.generation
    }
}

/// A request for suggestions for the word currently observed by the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionRequest {
    pub document_id: DocumentId,
    pub project_id: ProjectId,
    pub block_id: BlockId,
    pub range: EditorSelection,
    pub word: String,
    pub document_revision: EditorRevision,
    pub project_dictionary: DictionaryRevision,
    pub global_dictionary: DictionaryRevision,
}

/// A dictionary revision that an implementation must reload.
///
/// `project` is `None` for the global dictionary and `Some` for a project's
/// dictionary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DictionaryReload {
    pub project: Option<ProjectId>,
    pub revision: DictionaryRevision,
}

/// A receiver of result batches for one spellcheck request.
pub type SpellcheckResultStream = EventStream<SpellcheckResult>;

/// The engine-neutral boundary implemented by the private offline spellcheck
/// runtime.
pub trait SpellcheckService: Send + Sync {
    fn available_languages(&self) -> AsyncResult<Vec<LanguageId>>;

    fn check(&self, request: SpellcheckRequest) -> AsyncResult<SpellcheckResultStream>;

    fn suggest(&self, request: SuggestionRequest) -> AsyncResult<Vec<SpellingSuggestion>>;

    fn cancel(&self, handle: SpellcheckHandle);

    fn reload_project_dictionary(
        &self,
        project: ProjectId,
        revision: DictionaryRevision,
    ) -> AsyncResult<()>;

    fn reload_global_dictionary(&self, revision: DictionaryRevision) -> AsyncResult<()>;
}

#[cfg(test)]
mod spellcheck_api_contract_tests;
