# Stage 4 — test simplification: storage and history

Scope reviewed (source inspection only): tests and inline contract tests in
`parchmint-project-format`, `parchmint-project-repository`, `parchmint-project-fs`,
`parchmint-history-api`, and `parchmint-history-git2`.

## Result

No high-confidence test deletion or scenario merge is justified. The apparent
duplication is mostly deliberate coverage of distinct failure timing, ownership,
or state transitions. Removing or combining those cases would make failures less
local and could lose required atomicity, locking, crash-recovery, path-security,
or History regression coverage.

## Narrow safe simplification candidate (mechanical only)

1. `crates/parchmint-project-fs/tests/native_repository.rs`,
   `opening_rejects_missing_and_corrupt_canonical_resources` (around lines
   183–220): factor the repeated “create fixture, drop lease, mutate one
   resource, reopen and assert error” setup into a private test helper accepting
   the mutation closure and expected error predicate. Keep the two call sites
   separate (missing manifest versus unsupported format control).

   This preserves both behaviors and their distinct error assertions while
   reducing fixture boilerplate. It is optional: the current two cases are
   already short enough that a helper may reduce readability more than it helps.

## Deliberate non-simplifications

- `parchmint-project-fs/tests/atomic_writer.rs`: the two
  `interrupt_and_reconcile` callers intentionally cover failure before the first
  replacement and after a partial replacement; they must remain separate.
  Delete/reconcile, target identity re-check, collision, and foreign-owner tests
  exercise distinct atomicity/path-security guarantees.
- `parchmint-project-repository/src/lib.rs` tests separately cover missing,
  locked, unsafe, missing-resource, interrupted-save, lazy-load, and lease-drop
  states; merging them would obscure the state machine and error contract.
- `parchmint-history-api/src/history_store_contract_tests.rs` keeps idempotency,
  named snapshots, ordering/cursors, filtering, restore non-mutation, failure
  isolation, validation, and complete-restore deletion as independent contract
  scenarios. These are unique API guarantees, not duplicate fixtures.
- `parchmint-history-git2/tests/native_history.rs` similarly separates identity,
  corruption/reinitialization, stale-lock ownership, offline operation, line
  ending normalization, maintenance budget, and restore semantics. The repeated
  `LockedProject` setup is supported by `tests/common`; merging scenarios would
  make stateful failures harder to diagnose.
- `parchmint-project-format/src/lib.rs` test loops already consolidate equivalent
  invalid-input examples while preserving the specific valid/canonical examples;
  no safe removal was found.

No production code or tests were modified.
