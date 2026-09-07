# `parchmint-spellcheck-en-us`

**Purpose:** Implement offline en-US spellcheck with bundled, project, and global
dictionaries. Spelling-engine types and dictionary formats remain private.

## Interface

`EnUsSpellcheckService` implements
[SpellcheckService](../parchmint-spellcheck-api/README.md) with injected project
and global dictionary stores. Every result carries the observed document and
dictionary revisions and request generation. Callers discard stale results.
See [lib.rs](src/lib.rs).

## Scheduling and dictionaries

The worker runs dictionary reloads and suggestions before checks. Checks run in
`Visible`, `RecentlyChanged`, then `Background` order, newest first within each
rank. At capacity, the queue drops its oldest lowest-priority check and ends that
stream without a result. Cancelled or superseded jobs cannot publish results.
The scheduler retains the newest request stamp per document.

The private `harper-core` dependency has default features disabled; see
[Cargo.toml](Cargo.toml) for its version. Its Apache-2.0 English dictionary is
compiled into the binary, so users need no runtime dictionary files or network.
ParchMint supplies tokenization, transposition preference, custom dictionaries,
revision checks, queue limits, and cancellation. Identical inputs produce the
same suggestion ranking.

Saved dictionary changes survive engine reload failures. The service reports
the failure and can retry; it never sends prose to a network service.
