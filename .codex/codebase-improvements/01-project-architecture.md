# Project architecture review

## Verdict

ParchMint has a sound local-first, ports-and-adapters architecture. The domain,
application, persistence contracts, native adapters, and Iced UI mostly point in
the right direction. Keep that architecture. The highest-priority correction is
a project-lifetime leak in the desktop composition graph; the current timing and
service-cluster patches also need adjustment before they are treated as finished.

The workspace contains 30 product crates, two test-support crates, and one CI
tool (`Cargo.toml`). The size is justified by the real storage and platform
boundaries, but `parchmint-desktop/src/production.rs` has become a 3,769-line
secondary application layer. It assembles services and also implements project
open, query conversion, persistence workflows, export routing, UI callbacks,
and test controls. Keep one desktop composition root, but split that file into
private responsibility-based modules as nearby work is changed.

## Boundaries and dependency direction

The main dependency direction is correct: authored rules live in
`parchmint-domain`; orchestration lives in `parchmint-application`; `*-api`
crates own service contracts; implementation crates own filesystem, Git,
SQLite, spellcheck, export, platform, and GUI library types. The desktop and
core CLI crates are the two executable composition roots
(`crates/parchmint-desktop/Cargo.toml` and
`crates/parchmint-core-cli/Cargo.toml`).

| Boundary | Current owner | Assessment |
| --- | --- | --- |
| Authored project state and commands | `parchmint-domain`, `parchmint-application` | Sound. UI and adapters receive ParchMint-owned values. |
| Canonical files and atomic replacement | `parchmint-project-format`, `parchmint-project-repository`, `parchmint-project-fs` | Sound. Canonical paths, root capabilities, a writer lease, and transaction reconciliation protect the durable boundary. |
| Save, recovery, and History | `parchmint-save`, `parchmint-recovery-*`, `parchmint-history-*` | Sound ordering, but persistence coordination has two owners; see below. |
| Session and window authority | `ProjectUiPorts` in `parchmint-ui-api` and `WindowCapability` in `parchmint-platform-api` | Strong. Work reacquires exact-generation authority and stale completions are rejected. |
| Native implementations | desktop composition plus `*-fs`, `*-git2`, `*-sqlite`, `*-native`, and `*-iced` crates | Mostly sound. `parchmint-history-git2` intentionally depends on `parchmint-project-fs` for its checked native root, so the adapter boundary is not fully independent. |
| Preferences and workspace state | `parchmint-preferences`, `parchmint-workspace-state` | Practical but inconsistent with the `*-api` pattern: each crate combines public contracts and file implementations, and `parchmint-ui-api` depends on both. Split them only if another implementation is planned. |

The major ownership ambiguity is persistence. `parchmint-editor-api::EditorPersistenceCoordinator`
owns the recovery journal, optional save coordinator, and revision frontier
(`crates/parchmint-editor-api/src/lib.rs:780`).
`parchmint-application::EditorPersistenceCoordinator` wraps it and adds queue
and status policy (`crates/parchmint-application/src/editor_persistence.rs:57`).
Both are stateful and both define save/recovery behavior. New persistence policy
should go into the application owner. Moving the lower coordinator is an API
migration and needs an explicit decision about whether recovery belongs to an
editor session or the whole project session.

## Composition and cross-component data flow

The desktop builds application-wide services once in `assemble_with_controls`
and project-scoped services in `ProductionProjectFilesystem::open`
(`crates/parchmint-desktop/src/production.rs:2021,2744`). Project open runs on a
blocking worker for normal launcher actions
(`crates/parchmint-ui-iced/src/native.rs:7428`). It acquires the project lease,
loads summary-first state, initializes History and recovery, starts the search
worker, creates the serial save coordinator, and finally exposes session-checked
`ProjectUiPorts`.

```text
Iced intent
  -> session-authorized UI port
  -> application/domain or editor owner
  -> recovery projection and revision vector
  -> per-project save worker
  -> atomic project-file transaction
  -> matching History checkpoint
  -> search refresh and UI completion
```

Project files remain the canonical authored state. History records completed
saved states; recovery can be newer; search and workspace state are rebuildable
or non-authored projections (`docs/architecture/architecture.md`). This is a
clear ownership model and should remain the basis for future work.

One lifecycle edge is wrong. `ProductionDictionarySource::register_project`
stores a strong `Arc<dyn ProjectSnapshotQuery>` in a process-wide map and has no
unregister path (`crates/parchmint-desktop/src/production.rs:858-877,2210`). The
query retains project persistence and search state, which in turn can retain
save, History, and worker resources after the project session closes. It also
retains the partial graph if a later step in `open` fails. Change the registry
to weak references with stale-entry pruning, or unregister the exact entry on
every success, close, and failure path. A weak registry is the safer bounded
fix because lookup must not own project lifetime.

## Reliability and security

The durable design is strong. `FsAtomicWriter` records and reconciles a
multi-file transaction; `ProjectSaveCoordinator` serializes and coalesces saves;
recovery receipts authenticate revision vectors; History uses a per-root gate;
and UI tasks carry session, request, revision, and generation identity. These
mechanisms support failure recovery without making auxiliary caches canonical.

Security boundaries are also concrete. `CanonicalRelativePath` rejects unsafe
portable paths, the filesystem adapter rejects symlink and reparse escapes,
external-open accepts only `ValidatedExternalIntent::https_url`, and clipboard
and dialog results remain explicitly untrusted
(`crates/parchmint-project-format/src/lib.rs:98`,
`crates/parchmint-platform-api/src/lib.rs:282-417`). Git has no remote transport,
SQLite is bundled, and release policy checks locked sources, licenses,
advisories, bundled artifacts, and SBOM inputs (`deny.toml`,
`packaging/release.md`).

Error handling weakens at the UI edge. Production and native UI code frequently
turn typed errors into `String`, including project queries and desktop callbacks
(`crates/parchmint-desktop/src/production.rs:276,2542-2727` and
`crates/parchmint-ui-iced/src/native.rs`). The UI therefore cannot reliably
distinguish stale authority, conflict, permission, corruption, retryable I/O,
and unavailable-service outcomes. Introduce a small UI-facing error category
first for open, save, recovery, and session failures; keep the detailed message
for local diagnostics.

## Performance and execution model

The intended shape is appropriate: documents open summary-first, save/search/
spellcheck have service workers, autosave coalesces in-flight work, and stale
results are discarded. Source inspection does not prove the p95/p99 and 2 ms UI
turn budgets in `docs/product/scale-and-performance.md`.

Two execution risks need measurement:

- `NativeDesktop::run_service_job` and `run_blocking_operation` create a new OS
  thread for each job (`crates/parchmint-ui-iced/src/native.rs:6767-6798`) even
  though save, search, and spellcheck already own workers. This can create thread
  churn under overlapping project activity. Do not replace it with a shared
  runtime without measuring concurrency and defining shutdown/cancellation.
- `parchmint-diagnostics::event` takes one global mutex, writes, and flushes the
  file for every event (`crates/parchmint-diagnostics/src/lib.rs:101-124`). The
  new timing patch adds an event for every blocking job, including operations
  labeled `ui`, and the log is append-only. That instrumentation can itself add
  UI latency and unbounded disk growth.

The timing labels are not currently reliable execution-context evidence.
Dialogs, appearance changes, recent-project writes, and project-open dictionary
reloads are often invoked inside `run_blocking_operation`, but several wrappers
use the fixed label `ui` (`crates/parchmint-desktop/src/production.rs:2217-2235,
2674-2717`). Instrument the actual worker entry and the Iced update turn instead
of assigning context at individual `block_on` call sites. Buffer or bound timing
output, and record thresholds or aggregates so measurement does not dominate the
budget it is measuring.

## Review of the current uncommitted code

| Change | Judgment | Implementer guidance |
| --- | --- | --- |
| `.github/workflows/ci.yml`: run `architecture verify` | **Keep after checker adjustment.** CI is the right enforcement point and the command is offline and reproducible. | Keep the workflow step once the rule below handles dependency aliases and has an explicit, reviewed adapter list. Running it on all three OS jobs is redundant but harmless. |
| `tools/parchmint-ci/src/main.rs`: dependency-boundary verifier | **Adjust.** The protected-crate rule is useful, but dependency keys can alias another package, build dependencies are ignored, and the fixed eight-adapter list silently misses every future adapter. It also documents only the desktop root even though the core CLI is another composition root. | Resolve `{ package = ... }` names, test target-specific and aliased dependencies, and make the approved roots/adapters an explicit reviewed policy. Keep dev-dependencies excluded only as a documented test exception. |
| `crates/parchmint-diagnostics/src/lib.rs`: `timing` | **Adjust before keeping.** The fields avoid prose and paths, but synchronous per-event flush and append-only retention make it unsafe as high-volume performance telemetry. | Add bounded/rotated output and an off-UI buffered path or thresholded aggregation. Preserve best-effort behavior. |
| `crates/parchmint-desktop/src/lib.rs`: timed startup and recent-project calls | **Adjust.** Measuring these waits is useful, but `ui` is not necessarily the executing thread and the logger adds work after the wait. | Time actual startup/worker/update boundaries. Keep named operations, but derive context from the executor. |
| `crates/parchmint-desktop/src/production.rs`: timed `block_on` calls | **Adjust.** The helper is mechanically safe for return values, but the call-site labels are misleading and coverage is selective. | Centralize timing at executor boundaries. Do not use these records to claim PERF-006 compliance. |
| `crates/parchmint-ui-iced/src/async_service_feeds.rs`: time `BlockingServiceJob::run` | **Keep the measurement point, adjust the sink.** This location accurately measures worker job duration. | Retain it only with bounded, non-blocking diagnostics and tests that timing does not change job results. |
| `crates/parchmint-ui-api/src/lib.rs` plus desktop cluster construction | **Remove or make private; do not keep the current public shape.** Four new public raw-service bundles and a second constructor add 100+ lines for one production call. They do not reduce dependencies or authorization risk, and every new capability still changes a bundle and its caller. | With only one current construction site, keep the existing constructor or use a private desktop-local assembly helper. Revisit public clusters when a second UI/composition root proves the need. If clusters remain, expose one construction API rather than both. |

## Recommended next work

Fix the bounded lifetime and guardrail problems first; defer ownership and
runtime changes until their contracts and measurements are agreed.

### Safe bounded changes

1. Make the dictionary-source project registry non-owning and add close and
   failed-open lifetime tests.
2. Harden the dependency guard, then keep the CI step.
3. Rework timing around real executor boundaries and bound diagnostics storage;
   do not retain the current fixed `ui` labels.
4. Remove or privatize the partial UI service-cluster API.
5. Split `production.rs` into private `composition`, `project_session`,
   `workflow_adapters`, and `native_callbacks` modules when those areas are next
   edited. Preserve the current public API and graph behavior.
6. Add UI-facing error categories to open/save/recovery/session flows without a
   workspace-wide error rewrite.

### Decisions that require user discussion

- Choose one owner for recovery/save frontier policy and migrate the duplicate
  `EditorPersistenceCoordinator` types together with their contract tests.
- Decide whether a bounded shared blocking executor should replace transient UI
  worker threads only after concurrency, shutdown, and cancellation evidence is
  collected.
- Split preferences/workspace contracts from file implementations, or split
  `parchmint-ui-api` from its application facade, only when a second client or
  implementation makes the added boundary useful.
- A new bootstrap crate is not justified now. Private modules inside
  `parchmint-desktop` address the immediate ownership problem without another
  workspace dependency.

## Deployment and evidence limits

ParchMint currently builds one native `parchmint` executable with a pinned Rust
toolchain and locked dependencies. CI checks Linux, macOS, and Windows; package
templates and a fail-closed evidence verifier exist, but signing, notarization,
minimum-version evidence, package assets, and lifecycle evidence are explicitly
missing (`packaging/release-inputs.toml`). The repository therefore has a sound
build and release-policy shape, not a completed deployment pipeline.

This review inspected source, manifests, CI, packaging policy, and the current
diff only. It did not build, run tests, execute Cargo metadata, launch the UI,
measure latency or memory, or validate native packaging. Runtime thread counts,
cross-platform filesystem behavior, log overhead, and release readiness remain
unverified.
