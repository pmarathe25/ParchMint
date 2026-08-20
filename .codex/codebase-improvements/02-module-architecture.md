# Module architecture review

ParchMint's crate graph has clear intended direction: contracts and application policy sit below the desktop composition root, while filesystem, Git, SQLite, Iced, and platform implementations sit at the edge. The restarted Stage 1 work strengthens that rule and fixes the most direct project-session retention issue. The next work should concentrate on two stateful seams and one repeated worker boundary; splitting crates is not yet justified.

## What Stage 1 changed

**Dependency policy is now a useful guardrail.** `tools/parchmint-ci/src/main.rs` classifies every crate under `crates/`, rejects an unreviewed package, and collects normal, build, and target-specific production dependencies (`verify_dependency_boundaries`, lines 121-242). It permits concrete adapters only in the two named composition roots, with two visible UI exceptions. This is substantially stronger than a name-only adapter list. The rule deliberately excludes development dependencies, which is appropriate while contract tests construct adapters as fixtures.

**The process-wide dictionary source no longer retains closed sessions.** `ProductionDictionarySource` stores `Weak<dyn ProjectSnapshotQuery>` and removes an expired entry when lookup fails (`crates/parchmint-desktop/src/production.rs:856-905`). The Stage 1 test verifies that dropping the only live query releases it and that a failed lookup prunes its map entry (`production.rs:3727-3759`). This is the right ownership direction: a shared spellcheck dependency may observe a project but must not keep it alive.

## 1. Project persistence still has two public coordinators

**Conclusion:** save and recovery policy is split across two stateful coordinators, so a change to revision or recovery semantics has two owners.

`parchmint-editor-api::EditorPersistenceCoordinator` owns a recovery journal, optional save coordinator, and recovery frontier (`crates/parchmint-editor-api/src/lib.rs:782-1119`). `parchmint-application::EditorPersistenceCoordinator` wraps it, then adds a separate save queue and user-visible save/recovery status (`crates/parchmint-application/src/editor_persistence.rs:60-411`). `ProjectPersistenceCoordinator` adds a third state owner for canonical resources, pending saves, pending recovery acceptance, and workflow serialization (`crates/parchmint-application/src/project_persistence.rs:261-300`). It calls into the wrapper to persist projections and to submit/acknowledge saves (`project_persistence.rs:325-617,620-672`).

The API crate therefore contains production persistence coordination and imports both `parchmint-recovery-api` and `parchmint-save`; the application crate must understand the editor API's save/recovery frontier to add project-level policy. The layering works today, but the name collision makes the boundary hard to explain and invites new policy into the editor API.

**Major decision required:** decide whether recovery and revisioned save are document-session policy or project-session policy. The current project-wide data—canonical files, History checkpointing, structural edits, and recovery acceptance—supports project-session ownership.

**Implementer guidance after that decision:** keep editor-facing values such as `CanonicalProjection`, document revisions, and `EditorPersistenceError` in `parchmint-editor-api`. Move the stateful coordinator currently in that API into `parchmint-application` (or a new persistence crate) behind narrow editor/recovery traits; then rename the application wrapper to describe its actual project role. Move public exports and contract tests in one migration. Do not make this a small refactor: every construction path, recovery replay test, and save acknowledgment must move together.

## 2. Blocking work has four execution paths, so lifecycle and timing policy can drift

**Conclusion:** `parchmint-ui-iced` owns several independent thread-launching paths. Stage 1 instruments two, but the export and search paths bypass that instrumentation and all paths share no admission or shutdown policy.

The native UI directly creates workers for: the preference listener (`crates/parchmint-ui-iced/src/native.rs:1520`), generic service jobs (`6768-6784`), arbitrary blocking operations (`6786-6808`), export (`6810-6903`), and search (`6905-6921`). The two generic helpers now time work through `parchmint_diagnostics::timing`; the export and search workers do not. All five workers are detached OS threads. Session capability checks prevent a stale worker from starting a port call, but cannot cancel a call already in progress or give the owner a joined/shutdown handle.

This matters because the Stage 1 timing API groups results by operation and context (`crates/parchmint-diagnostics/src/lib.rs:153-216`). Its data is bounded, but reported operation coverage is an implementation convention, not an enforced executor boundary. A future direct `thread::spawn` can silently omit timing and any later cancellation/accounting rule.

**Safe bounded change:** add one private `NativeDesktop` worker-launch helper that owns thread naming, elapsed-time recording, and result delivery. Route the existing generic, search, and export launches through it. Preserve the current detached-thread behavior and the export progress stream. Add focused tests around the helper's completion and spawn-failure messages; no pool is needed for this change.

**Major decision required before further work:** choose whether ParchMint needs bounded concurrency, cooperative cancellation, or orderly joining at close. That needs product limits and service-specific cancellation support, especially because export currently exposes progress through a distinct stream. A platform-level executor or a new async runtime would be a migration, not a follow-up to instrumentation.

## 3. The UI boundary protects session access but still erases useful error categories

**Conclusion:** lower UI layers distinguish stale sessions and service failures, but the native reducer frequently turns both into `String` before selecting UI behavior.

`ProjectUiPorts::access` returns `StaleProjectSession`, and `ProjectUiAccess` rechecks the exact capability for each port use (`crates/parchmint-ui-api/src/lib.rs:628-855`). `async_service_feeds` preserves several categories in `ServiceFeedError`, including `StaleSession`, stale search generation, invalid service data, and unsupported boundaries (`crates/parchmint-ui-iced/src/async_service_feeds.rs:145-220`). At the native boundary, `run_service_job`, `run_blocking_operation`, export, search, and many individual task closures use `error.to_string()` (`crates/parchmint-ui-iced/src/native.rs:6768-6921` and call sites throughout that file). Production adapters similarly construct `ProjectQueryError` from display strings (`crates/parchmint-desktop/src/production.rs:274,297,1180,1651,1655`).

The UI can display a message, but it cannot reliably decide that a stale completion should be ignored while a permission or recovery failure should offer a distinct action. String matching would make the capability contract fragile.

**Safe bounded change:** define a compact native-task outcome for the paths that already have special recovery behavior: `StaleSession`, `Canceled`, `Unavailable`, and `Failed { message }`. Convert `StaleProjectSession` and the corresponding `ServiceFeedError` variants before spawning work, then let the reducer silently discard stale/canceled results. Keep implementation-specific error text in the diagnostic event and in `Failed`.

**Do not do yet:** a workspace-wide error hierarchy. The project, History, export, platform, and editor APIs already have typed errors with different recovery semantics. Add categories only where the UI has a documented different response, then expand from evidence.

## Smaller lifecycle follow-up: prune dictionary registrations on registration

The Stage 1 weak-reference change releases project queries correctly, but an expired key remains until somebody calls `project_words` for that same project. `register_project` only inserts (`crates/parchmint-desktop/src/production.rs:869-874`); the only prune is the lookup failure branch (`879-895`). Repeatedly opening distinct projects and never calling the dictionary source again for them can leave a small but unbounded map of expired keys.

**Safe bounded change:** before inserting, call `retain` on the registry to remove entries whose `Weak::upgrade` fails. This stays local to `ProductionDictionarySource`, keeps the current lazy lookup behavior, and needs one test that registers two expired project IDs before a third live one. An explicit project-close unregister hook is unnecessary unless another shared service needs close-time notification.

## Priority and limits

1. Centralize the existing UI worker launches; it is bounded and makes Stage 1 timing coverage dependable.
2. Add the narrow native-task categories at stale-session and cancellation boundaries.
3. Treat persistence consolidation as an agreed migration, after naming the intended project-session owner.
4. Prune expired dictionary keys opportunistically when touching that registry.

This review inspected current source, manifests, and the uncommitted Stage 1 diff only. No builds, tests, Cargo metadata, runtime traces, or memory measurements were run. The worker-concurrency and registry-growth risks are structural inferences from ownership and call paths; their production frequency is not established by this review.
