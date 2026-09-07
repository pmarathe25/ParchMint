# `parchmint-spellcheck-api`

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

## Interface

`SpellcheckService` checks revisioned text snapshots and returns issues and
suggestions through ParchMint values. Dictionary interfaces separate the
global preference dictionary from each project dictionary.

See [the source](src/lib.rs) for method signatures.

`SpellcheckOperation` preserves typed request, worker, and dictionary errors.
An empty result stream means cancelled, superseded, or evicted work.

`SpellcheckRequest::accepts` reports whether a result belongs to that exact
request, so callers can discard stale results.

Results use ParchMint text ranges, normalized words, categories, and ranking
values. The implementation converts its engine's values before returning them.

## Implementation boundary

`parchmint-spellcheck-en-us` owns scheduling, bounded queues, cancellation, and
the private spelling engine. This crate documents only the ParchMint contract;
results retain the revision and generation semantics described above.
