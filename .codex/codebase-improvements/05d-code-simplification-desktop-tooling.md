# Stage 5D — Desktop and tooling code-simplification audit

## Scope and result

Inspected production/configuration code in `parchmint-desktop`,
`parchmint-core-cli`, `parchmint-diagnostics`, `tools/parchmint-ci`,
packaging/release configuration, and workspace-root configuration, including
the current Stage 1–4 worktree changes. Inspection was source/config only;
no builds, tests, metadata commands, or heavy scans were run. No high-
confidence simplification is recommended in this group (0 changes).

## Rejected apparent simplifications

* `crates/parchmint-desktop/src/lib.rs:515-844` contains several small
  authority and lifecycle helpers (`register_project`, `begin_final_save`,
  `resolve_final_save`, `unregister`, and `accepts`). They look like
  forwarding layers, but each preserves a distinct window/session generation
  check or final-save transition. In particular, the current close ordering
  at `:721-768` makes the UI close notification happen before registry removal
  and rejects stale callbacks; merging these paths would risk lifecycle
  regressions.
* `crates/parchmint-desktop/src/production.rs:2750-2820` assembles shared
  services, while `:2260-2340` creates per-project services. The repeated
  `Arc`/trait-object composition is the production dependency boundary and
  supports test seams; collapsing it into a global or one larger service would
  weaken explicit dependency policy and project-session ownership.
* `crates/parchmint-diagnostics/src/lib.rs:44-263` has separate sink locking,
  bounded writes, no-follow opening, and timing aggregation. These are not
  redundant layers: bounded rotation and the open descriptor protect the log,
  while `timing` deliberately avoids synchronous flushes. Removing the
  target-specific `libc`/`windows-sys` dependencies or folding the helpers
  together would remove the symlink/reparse-point safety boundary introduced
  in the current changes.
* `tools/parchmint-ci/src/main.rs:126-291` deliberately resolves workspace
  aliases, target-specific production dependencies, and reviewed package
  classifications separately. The constants and checks are release/dependency
  policy evidence, not dead configuration. Likewise, the repeated checks in
  `:295-507` and `:899-1000` distinguish unresolved inputs from a ready release
  and bind evidence to paths and hashes; combining them would make the
  fail-closed lifecycle less explicit.
* `crates/parchmint-core-cli/src/lib.rs:187-270` keeps parsing, cancellation,
  execution, and machine-output emission separate. `--cancel` must short-circuit
  after syntactic parsing, while `emit` preserves the stable schema and exit
  codes; replacing these with a single command dispatcher would alter CLI
  policy rather than simplify implementation.
* `Cargo.toml`, `rust-toolchain.toml`, `deny.toml`, and
  `packaging/release-inputs.toml` contain settings that appear repetitive (for
  example pinned toolchain components and supply-chain allowlists), but each
  is consumed by a different local or CI/release boundary. No unused setting
  was confirmed by source/config references alone.

## Validation

Validation was limited to `rg`, manifest inspection, and line-level source
tracing, as required. No files besides this report were edited. No production
or policy change is proposed.
