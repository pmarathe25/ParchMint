# Stage 5A — Core/storage code-simplification audit

## Scope and result

Inspected production sources under `parchmint-domain`, `parchmint-application`,
`parchmint-editor-api`, `parchmint-editor-core`, `parchmint-save`,
`parchmint-recovery-fs`, `parchmint-project-format`,
`parchmint-project-repository`, `parchmint-project-fs`, and
`parchmint-history-api` (source/config inspection only). No production or test
code was changed. No high-confidence simplification is justified in this
group.

## Rejected apparent simplifications

* `parchmint-project-repository/src/lib.rs:136-142,193-269` keeps both the
  in-memory `active` project path and per-path `leases`. `active` is read by
  `load_document`, while `leases` independently enforces the open-session
  invariant and is cleared by `Lease::drop` (`:98-110`). Removing either field
  would change lazy document lookup or allow reads after the lease closes.
* `parchmint-project-fs/src/lib.rs:1662-1711,1784-1889` appears to duplicate
  the opened project in `ActiveProject`, but `active_root()` is a production
  composition seam and `load_document` needs both the scanned document index
  and the retained root capability. The cache is therefore not dead state;
  collapsing it would either reacquire a lock or remove the lazy-load contract.
* The small typed result wrappers in
  `parchmint-project-repository/src/lib.rs:335-394` (`CommitReceipt`,
  `ValidationReport`, `Reconciliation`, and `Abandonment`) are not redundant
  booleans in practice: their accessors are consumed by save, recovery, CLI,
  and filesystem implementations. They form the repository contract's typed
  transition/result boundary and should not be flattened.
* `parchmint-history-api/src/lib.rs:374-409` default methods for unsupported
  reinitialization and resource reads are deliberate compatibility behavior
  for providers implementing the trait; deleting them would expand required
  implementation surface and break the contract-test fixture.

## Validation

Validation was limited to symbol/reference tracing with `rg` and line-level
source inspection, per the assignment. Builds, tests, metadata scans, and
production edits were intentionally not performed.
