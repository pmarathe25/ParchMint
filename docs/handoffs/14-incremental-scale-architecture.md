# Stage 14 handoff: incremental scale architecture

Status: the incremental architecture and enforced Linux performance gates are
implemented. The available Linux reference host passes the 10,000-node /
10.02-million-word budgets. Stage acceptance is not claimed for the required
second desktop OS profile, because no Windows or macOS host was available, or
for a successful worst-case EPUB/DOCX peak-RSS capture; those are called out
under remaining evidence.

Verification working tree: based on
`ba0229fa2e1c2db7ab542136ca8f9355f5e5bd15` plus the changes described here.
Reference host: Intel Core i7-8550U, 4 cores/8 threads, 7.1 GiB usable RAM,
local ext4 SSD, Linux 7.0.0-27-generic x86-64. Toolchain: Rust/Cargo 1.97.1,
CMake 4.3.4, and repository Qt 6.8.3.

## Format, API, and ADR state

- No canonical Markdown, TOML, recovery-journal, asset-catalog, or compile
  preset version changed.
- `.parchmint/pending-save-v1` is new versioned operational recovery state. It
  is non-authoritative and is removed after acknowledgement or recovery.
- [ADR 0014](../architecture/0014-incremental-transactions-and-revisions.md)
  records in-place domain commands, dirty-resource transactions, serial
  structural persistence, revision taxonomy, delta models, and cancellation.
- New Rust integration surfaces include `DirtySet`, `SaveMetrics`, lazy
  `DocumentBodySnapshot`, `ProjectSavePlan`, revisioned search progress/count
  rows, typed `OutlineDelta`, and UTF-16 editor deltas. The bridge adds bounded
  row JSON, independent revision properties, delta signals, pane count/delta
  invokables, and diagnostic FFI-byte accounting.
- Search remains relevance ordered. To return a bounded first result on dense
  10M-word matches, relevance is stable within a fixed 4,096-candidate window
  instead of requiring a global rank sort before the first row.

## Domain and transaction design

`Project::execute` now prevalidates one command, mutates the live project, runs
event-local invariant checks, and returns an exact inverse. It no longer clones
or fully validates the project for each command. `Project::validate` remains an
explicit open/migration/support/debug oracle. Node ancestry, graph validation,
projection traversal, duplication, and compile traversal use explicit stacks
and visited/depth state; 20k/30k-depth valid and invalid fixtures complete
without consuming the process stack.

Storage maps events to canonical resources:

| Change | Dirty resources |
| --- | --- |
| Rename, synopsis, flags | One Markdown document |
| Reorder | `outline.toml` |
| Reparent/trash/restore | `outline.toml`, affected subtree documents, affected tombstone |
| Styles/presets | `styles.toml` |
| Body save | One Markdown document |

Document locations are cached at open and recomputed only for an affected
subtree. Open parses bounded front matter but leaves every Markdown body behind
a thread-safe lazy handle.

For each structural command the project owner freezes the dirty write set and
bounded inverse, applies the model delta, and submits a plan to one serial save
worker. Outline TOML serialization, backup publication, `fsync`, replacement,
and cleanup happen on that worker. A plan first stores old bytes and a durable
transaction record, publishes the pending directory, mutates the small file
set, and removes the marker only after all changes succeed. A failure restores
old files, removes newly created files, and rolls back that optimistic command
and all later queued commands. Transitions wait for the queue, and document
canonical saves do not race structural plans.

The fault test injects failure after the first canonical mutation and proves
that both the live graph and a reopened project retain the complete old state.

## Index, counts, and revision protocol

- Initial index/open work reopens canonical headers and lazy bodies on the
  search worker, so project open performs neither a body scan nor a full
  owner-thread `Project` clone.
- SQLite rebuilds publish 64-document transactions with revision, completed,
  total, count-row batches, and a terminal state. Normalization checks
  cancellation every 4,096 characters. A structural plan cancels an unfinished
  revision and starts its replacement only after canonical acknowledgement.
- `RebuildNeeded`, `Indexing`, `Ready`, and `Unavailable` are explicit. Search
  and totals never trigger a synchronous cold rebuild or present a zero-prefix
  as complete.
- Editor changes carry UTF-16 position/removal, the inserted fragment, and
  affected blocks. Rust updates the live body and derives count changes from
  the changed boundary context. Document counts and subtree aggregates are
  cached; signed deltas propagate through ancestors.
- Content, structure, selection, and presentation revisions are independent.
  Background jobs carry only the revision they depend on.

## Model and FFI protocol

Normal binder operations publish stable-ID insert/remove/move/data deltas.
`Reset` is reserved for an explicit filter/sort projection change or recovery
from a failed optimistic transaction. `OutlineModel` caches one complete row
payload and serves every Qt role locally; `CardsModel` consumes that cache.

The Qt editor defers `QTextDocument::contentsChange` delivery to the next event
turn, because Qt reports replacement ranges before the inserted fragment is
readable. It queues the small range records, extracts only those fragments, and
emits them in order. The lifecycle smoke found this boundary and now asserts
that one editor replacement uses at most 4,096 recorded FFI bytes. QML no
longer sends the full document or asks Rust to rescan it on every keystroke.

## Compile/export boundaries

- Project and Markdown traversals are iterative. Parser scans, block and inline
  rendering, asset reads, CRC, and archive writes observe one cooperative
  cancellation token.
- Combined Markdown and plain text render directly to the atomic temporary
  destination one block at a time.
- Asset reads and CRC use 64 KiB chunks. CRC32 uses a lookup table rather than
  the previous per-bit implementation.
- The deterministic store-only archive writer checks all arithmetic and emits
  ZIP64 local/central metadata, ZIP64 EOCD, and the locator when classic ZIP
  limits are crossed. A 65,536-entry fixture validates the path.
- Cancellation/failure drops temporary output and archive state before a
  destination replacement is acknowledged.

## Before/after measurements

Release measurements on the reference host:

| Boundary | Before | Stage 14 |
| --- | ---: | ---: |
| Canonical 10k/10.02M open | Eager body parse; no passing bounded baseline | 715.766 ms, 0 bodies loaded |
| Rename owner / persistence | Synchronous transaction | 0.090 ms / 76.404 ms |
| Metadata owner / persistence | Synchronous transaction | 0.061 ms / 70.943 ms |
| 10k reorder owner / persistence | 169.173 ms synchronous | 0.425 ms / 187.214 ms |
| Reorder write set | Not recorded | 1 file, 2,320,317 bytes |
| 250k Rust lifecycle | Failed the former 1M delimiter-scan ceiling | load 203.044 ms; UI delta 0.605 µs; journal 57.863 ms; canonical save 146.760 ms |
| 250k native editor | Full dirty text was the interaction payload | load 25 ms; typing p95 3.985 ms, p99 4.408 ms; 500/500 deltas |
| Lifecycle editor FFI payload | Full document on every change | 236 bytes for the measured replacement/model delta |
| Dense first search | 525 ms global rank sort | 95.724 ms bounded relevance; rebuild 1.191 s |
| Index peak RSS | Not gated | 69,140 KiB |
| 10M-word compile cancellation | Not enforced | 0.454 ms |
| 10k projection build | Whole reset was the consumer protocol | 1.505 ms build; typed consumer deltas afterward |

The canonical scale storage gate also recorded one-file write sets of 5,093
bytes for rename, 5,156 bytes for synopsis/include metadata, and 5,168 bytes
for a single-body save. The raw `outline.toml` replacement is necessarily
proportional to the canonical outline file, but serialization and disk work no
longer occupy the UI owner.

## Enforced gates and exact verification

The previously ignored stress measurements use `cfg_attr(debug_assertions,
ignore)`: they remain cheap in debug and are mandatory in release. The Tuesday
nightly runs the full Rust budget set and the native editor benchmark, retaining
both outputs for 90 days.

Commands run on this Linux host:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --exclude parchmint_bridge --all-targets --offline -- -D warnings
cargo test --locked --workspace --exclude parchmint_bridge --offline
TMPDIR=$PWD/target/stage14-tmp QMAKE=$PWD/.toolchains/qt/6.8.3/gcc_64/bin/qmake CMAKE_PREFIX_PATH=$PWD/.toolchains/qt/6.8.3/gcc_64 cargo check --locked -p parchmint_bridge --offline
TMPDIR=$PWD/target/stage14-tmp cargo test --locked -p parchmint-app -p parchmint-markdown -p parchmint-index -p parchmint-compile -p parchmint-storage --release --offline -- --nocapture
TMPDIR=$PWD/target/stage14-tmp cmake --build build-stage14 --parallel 2
TMPDIR=$PWD/target/stage14-tmp ctest --test-dir build-stage14 --output-on-failure
QT_QPA_PLATFORM=offscreen LD_LIBRARY_PATH=$PWD/.toolchains/qt/6.8.3/gcc_64/lib build-stage14/app/cpp/parchmint-editor-benchmark -o -,txt
cmake --build build-stage14 --target qmllint --parallel 2
```

Results: formatting and strict clippy passed; Rust workspace tests passed; the
release performance suite passed with no ignored tests; bridge check passed;
the native build passed; CTest passed 4/4; the editor benchmark passed 4/4; and
the QML lint target exited successfully. The established GNU ld.bfd advisory
and existing resource-singleton/unqualified QML lint warnings remain.

## Platform variance and remaining headroom

- Local measurements cover Linux/ext4 only. Windows/macOS filesystem latency,
  peak RSS, and cancellation must still be profiled on real hosts. The nightly
  Linux artifacts are ready for trend comparison; they are not a substitute
  for the required second OS evidence.
- Markdown/plain export is streaming, but successful EPUB/DOCX generation
  still retains the compile IR and a complete checked archive buffer. The
  10M-word cancellation path is proven; a successful worst-case EPUB/DOCX
  peak-RSS capture and spill strategy remain before claiming that memory gate
  on every format.
- Search hierarchy context still materializes an ancestry string for each
  indexed document. Graph operations are iterative, but a document at every
  level of an adversarial deep tree needs a dedicated interval-based subtree
  index before claiming linear derived-index space at arbitrary depth.
- Compile input still clones project metadata for an explicit export snapshot.
  Body bytes are shared/lazy and ordinary editing/open/search interactions no
  longer clone the project, but a persistent immutable graph representation
  would remove this final full metadata snapshot.

Recommended next task: replace ancestry strings with preorder/subtree intervals
in the disposable index, then add successful 10M-word EPUB/DOCX peak-RSS gates
on Linux and one Windows/macOS profiler host.
