# Stage 5B — Services code-simplification audit

## Scope and result

Inspected production sources in `parchmint-search-api`,
`parchmint-search-sqlite`, `parchmint-spellcheck-api`,
`parchmint-spellcheck-en-us`, `parchmint-export-api`, `parchmint-export-html`,
`parchmint-contracts`, `parchmint-preferences`, and
`parchmint-workspace-state`, including the current Stage 3 changes. Source
and configuration inspection only; no production code was changed. No
high-confidence simplification is justified in this group.

## Rejected apparent simplifications

* `parchmint-search-sqlite/src/lib.rs:87-130,208-290,525-590` keeps separate
  worker ownership, operation serialization, rebuild status, background task
  ownership, and cancellation generations. These are distinct concurrency and
  lifecycle contracts; collapsing them would risk changing cancellation,
  stale-generation delivery, or worker shutdown behavior.
* `parchmint-spellcheck-en-us/src/lib.rs:861-985` now performs project-keyed
  dictionary lookup in both `contains` and `suggestions`. The `project_id`
  threading introduced by the current Stage 3 fix is essential project
  isolation, not redundant plumbing, and must not be removed.
* `parchmint-export-html/src/lib.rs:121-240` deliberately maintains a
  normalized comparison form while returning the newline-normalized original
  CSS. The escape/comment canonicalization is the security boundary added by
  Stage 3; merging it with emitted CSS or removing the lexical pass would
  re-open obfuscated URL/script bypasses.
* `parchmint-export-api/src/lib.rs:248-320,334-410` stores the checked target,
  source revisions, and resolved settings in the immutable plan. These fields
  are consumed by format implementations and validation; removing the
  apparently small scope/settings wrappers would weaken the export contract or
  alter inherited-setting and mixed-revision behavior.
* `parchmint-preferences/src/lib.rs:230-407,409-490,596-745` separates durable
  file storage, revision-checked coordination, and appearance publication.
  The delegation methods on `AppearanceController` are public trait-backed
  entry points, while `initialized`, revision state, and subscriber lists each
  enforce observable lifecycle/concurrency behavior.
* `parchmint-workspace-state/src/lib.rs:147-157,276-319,342-400` intentionally
  distinguishes invalid-file warnings from storage errors and retains a
  revision read during serialized durable replacement. Flattening either
  result path or removing the operation lock would change recovery diagnostics
  or concurrent-write behavior.
* `parchmint-contracts/src/lib.rs:90-180,220-290` and
  `src/generated.rs` use the generated types, schema manifest, and canonical
  re-encoding as compatibility checks. The apparently unused `_canonical`
  bindings intentionally force serialization validation; deleting them would
  remove a contract assertion rather than dead code.

## Validation

Validation was limited to `rg` symbol/reference tracing and line-level source
inspection, per the assignment. Builds, tests, metadata scans, and heavy scans
were intentionally not performed.
