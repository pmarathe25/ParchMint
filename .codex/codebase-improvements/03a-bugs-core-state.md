# Core state bug audit

## Confirmed bug: acknowledging one recovery batch erases other document revisions

Evidence: `crates/parchmint-editor-api/src/lib.rs`, `EditorPersistenceCoordinator::acknowledge_recovery` (lines 961–963) replaces the complete frontier map with `durable.batch.revision_vector().documents`. A `RecoveryBatch` intentionally contains only the documents changed by that batch (`RecoveryBatch::revision_vector` in `crates/parchmint-recovery-api/src/lib.rs`), not a complete document frontier.

Trigger: start with a base containing document A at revision 5 and document B at revision 7. Persist and acknowledge a batch changing only A to revision 6. The coordinator now retains only `{A: 6}`; B's revision 7 is gone. Persisting B's next projection computes `previous` as the default revision 0 in `persist_projection_with_document_hash`, creates range 1..1, and `RecoveryJournal::append` rejects it as non-consecutive (expected B revision 8). Thus an unrelated document cannot be saved after another document's recovery batch until the process is rebuilt/reconciled, and the in-memory frontier no longer describes the durable state.

The same lossy replacement occurs during crash recovery in `reconcile_recovery` (lines 975–981) and in the replay-prefix loop of `resume_recovery_acknowledgement` (lines 1048–1051), so restarting/reconciling can reproduce the problem even when the journal is valid.

Minimal fix: merge each batch's document endpoints into the existing frontier map (`extend`/insert touched documents), preserving entries for untouched documents. Apply that merge in all three locations. Focused regression test: initialize a coordinator/base with two document revisions, acknowledge a batch for only A, then persist/acknowledge B's next projection and assert success plus B's revision remains 8; add the analogous reconcile path test with two accepted batches.

## Uncertain observations

None reported. The finding above follows directly from the partial-map contract and the subsequent `unwrap_or_default().next()` calculation; no build or test execution was performed per the audit scope.
