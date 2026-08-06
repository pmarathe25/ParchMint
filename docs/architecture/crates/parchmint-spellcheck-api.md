# `parchmint-spellcheck-api`

## What it does

`parchmint-spellcheck-api` defines offline en-US spellcheck. The editor sends
small text ranges for checking and receives misspellings and ranked suggestions
asynchronously. The public API contains no spelling-engine or operating-system
types.

The API defines a ParchMint `LanguageId` so a later release can add languages
without changing the editor or application boundary. V1 exposes only `en-US`.

## How it works

```text
visible or recently changed text
  -> small ranges with nearby word context
  -> request tagged with text and dictionary revisions
  -> offline spellcheck worker
  -> result batches
  -> discard results for old text or dictionary revisions
  -> update spelling underlines
```

Opening a document checks visible text first. Suggestion requests name the word
and document revision under the pointer or caret. A correction edits the
document. Dictionary actions update either the project or global dictionary.

## Public API

```rust
#[non_exhaustive]
pub enum LanguageId {
    EnUs,
}

pub trait SpellcheckService: Send + Sync {
    fn available_languages(&self) -> AsyncResult<Vec<LanguageId>>;

    fn check(
        &self,
        request: SpellcheckRequest,
    ) -> AsyncResult<SpellcheckResultStream>;

    fn suggest(
        &self,
        request: SuggestionRequest,
    ) -> AsyncResult<Vec<SpellingSuggestion>>;

    fn cancel(&self, handle: SpellcheckHandle);

    fn reload_project_dictionary(
        &self,
        project: ProjectId,
        revision: DictionaryRevision,
    ) -> AsyncResult<()>;

    fn reload_global_dictionary(
        &self,
        revision: DictionaryRevision,
    ) -> AsyncResult<()>;
}

pub struct SpellcheckRequest {
    pub language: LanguageId,
    pub document_revision: EditorRevision,
    pub blocks: Vec<RevisionedTextRange>,
    pub project_dictionary: DictionaryRevision,
    pub global_dictionary: DictionaryRevision,
    pub generation: SpellcheckGeneration,
}
```

Results use ParchMint text ranges, normalized words, categories, and ranking
values. The implementation converts its engine's values before returning them.

## Implementation boundary

`parchmint-spellcheck-en-us` owns scheduling, bounded queues, cancellation, and
the private spelling engine. This crate documents only the ParchMint contract;
results retain the revision and generation semantics described above.
