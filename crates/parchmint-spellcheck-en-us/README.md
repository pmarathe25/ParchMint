# `parchmint-spellcheck-en-us`

`parchmint-spellcheck-en-us` implements `SpellcheckService` for offline `en-US`
writing. It combines a bundled dictionary with the project's dictionary and the
user's global dictionary.

The crate keeps spelling-engine types and dictionary formats out of the editor,
application, and project model.

## How it works

```text
revisioned text ranges
  -> bounded spellcheck worker
  -> bundled + project + global dictionaries
  -> ranked ParchMint suggestions and text ranges
```

Each request carries the document and dictionary revisions it observed. The
caller discards a result when any of those revisions is old.

## Interface

`EnUsSpellcheckService` implements
[`SpellcheckService`](../parchmint-spellcheck-api/README.md) using the bundled
dictionary and injected project and global dictionary stores.

See [the source](src/lib.rs) for method signatures.

The constructor and contract methods use ParchMint-owned values. No spelling
engine or operating-system spellcheck type leaves this crate.

## Implementation

The scheduler keeps the newest request stamp per document, suppresses results
for cancelled or superseded work, and schedules visible ranges ahead of older
background work.

The worker dequeues by rank: dictionary reloads and suggestion requests run
before checks, and checks run `Visible`, then `RecentlyChanged`, then
`Background`, newest first within a rank. When the queue exceeds its fixed
capacity, the lowest-priority, oldest check job is dropped and its stream ends
without a result. Results for cancelled or superseded handles are suppressed.
Every result reports the document and dictionary revisions and the generation
it observed. Suggestions use deterministic ranking for the same inputs.

The bundled Harper dictionary is compiled into the application binary; no
runtime dictionary files are needed. Project and global dictionary changes
remain saved when the private engine fails to reload; the service reports the
failure and can retry. The crate sends no prose to a network service.

## Engine selection

The implementation uses `harper-core` 0.29 with default features disabled. Its
Apache-2.0 licensed curated English dictionary is compiled into the dependency,
works without runtime files or network access, and provides fast lookup and
ranked fuzzy suggestions on every Rust desktop target. ParchMint adds
transposition preference, tokenization, revision checks, custom dictionaries,
queue bounds, and cancellation outside the private engine.
