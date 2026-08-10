# `parchmint-spellcheck-en-us`

## What it does

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

## Public API

The desktop application constructs the implementation and exposes it through
the contract crate:

```rust
pub struct EnUsSpellcheckConfig {
    pub bundled_dictionary: BundledDictionarySource,
    pub worker_limits: SpellcheckWorkerLimits,
    pub saved_dictionaries: Arc<dyn SavedDictionarySource>,
}

pub struct EnUsSpellcheckService {
    inner: Box<dyn PrivateSpellingRuntime>,
}

impl EnUsSpellcheckService {
    pub fn new(config: EnUsSpellcheckConfig)
        -> Result<Self, SpellcheckStartupError>;
}

impl SpellcheckService for EnUsSpellcheckService {
    // Implements the ParchMint-owned contract.
}
```

The constructor and contract methods use ParchMint-owned values. No spelling
engine or operating-system spellcheck type leaves this crate.

## Implementation

The scheduler keeps the newest generation per view, cancels obsolete jobs, and
puts visible ranges ahead of older background work:

```rust
struct Scheduler {
    current: HashMap<ViewId, SpellcheckGeneration>,
    queue: BoundedPriorityQueue<SpellcheckJob>,
}

fn schedule(scheduler: &mut Scheduler, request: SpellcheckRequest) {
    scheduler.cancel_older(request.view(), request.generation);
    scheduler.queue.push_visible_first(request.into_job());
}
```

The worker checks visible and recently changed ranges first. Its queue has a
fixed size, cancels obsolete jobs, and reports the text and dictionary revisions
with every result. Suggestions use deterministic ranking for the same inputs.

Bundled dictionaries and licenses ship with the application. Project and global
dictionary changes remain saved when the private engine fails to reload; the
service reports the failure and can retry. The crate sends no prose to a network
service.

## Engine selection

The implementation uses `harper-core` 0.29 with default features disabled. Its
Apache-2.0 licensed curated English dictionary is compiled into the dependency,
works without runtime files or network access, and provides fast lookup and
ranked fuzzy suggestions on every Rust desktop target. ParchMint adds
transposition preference, tokenization, revision checks, custom dictionaries,
queue bounds, and cancellation outside the private engine.

`spellbook` was not selected directly. Although its engine is small and its
Hunspell support is useful, its public runtime API does not bundle a dictionary,
its MPL-2.0 license is outside the current dependency allowlist, and scheduling,
cancellation, revision rejection, and dictionary persistence would still need
to be supplied separately. The selected Harper release has no runtime network
client or machine-learning dependency in this crate's normal dependency tree.
