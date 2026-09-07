# `parchmint-spellcheck-api`

**Purpose:** Define asynchronous offline spellcheck for en-US
through ParchMint text ranges, normalized words, categories, and ranking values.
Spelling-engine and OS types stay behind the implementation.

## Interface

`SpellcheckService` checks revisioned text snapshots and returns issues and
suggestions. Dictionary interfaces separate project words from global preference
words. `SpellcheckOperation` preserves typed request, worker, and dictionary
errors. An empty stream means cancelled, superseded, or evicted work.
See [lib.rs](src/lib.rs) for signatures and the language identifier.

## Request lifecycle

Opening a document checks visible text first. Requests contain bounded text
ranges with nearby word context and observed document and dictionary revisions.
Suggestion requests identify the word and revision at the caret or pointer.
`SpellcheckRequest::accepts` checks that a result belongs to the exact request;
callers discard stale results before updating underlines.

Corrections edit the document. Dictionary actions update the selected project
or global store. [spellcheck-en-us](../parchmint-spellcheck-en-us/README.md)
owns scheduling, bounded queues, cancellation, and the private spelling engine.
