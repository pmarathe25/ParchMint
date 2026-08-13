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

## Interface

The desktop application constructs the implementation and exposes it through
the contract crate:

```rust
pub struct EnUsSpellcheckConfig {
    pub bundled_dictionary: BundledDictionarySource,
    pub worker_limits: SpellcheckWorkerLimits,
    pub saved_dictionaries: Arc<dyn SavedDictionarySource>,
}

#[derive(Clone)]
pub struct EnUsSpellcheckService {
    inner: Arc<ServiceInner>,
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

The scheduler keeps the newest request stamp per document, suppresses results
for cancelled or superseded work, and schedules visible ranges ahead of older
background work:

```rust
struct SchedulerShared {
    state: Mutex<SchedulerState>,
    changed: Condvar,
    limits: SpellcheckWorkerLimits,
}

struct SchedulerState {
    queue: Vec<QueuedWork>,
    next_handle: u64,
    next_sequence: u64,
    newest: HashMap<DocumentId, RequestStamp>,
    cancelled: HashSet<SpellcheckHandle>,
    shutdown: bool,
}

impl SchedulerShared {
    fn enqueue(&self, work: Work) -> Option<SpellcheckHandle>;
    fn take_next(&self) -> Option<QueuedWork>;
    fn is_current(&self, handle: SpellcheckHandle, request: &SpellcheckRequest) -> bool;
}
```

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

`spellbook` was not selected directly. Although its engine is small and its
Hunspell support is useful, its public runtime API does not bundle a dictionary,
its MPL-2.0 license is outside the current dependency allowlist, and scheduling,
cancellation, revision rejection, and dictionary persistence would still need
to be supplied separately. The selected Harper release has no runtime network
client or machine-learning dependency in this crate's normal dependency tree.
