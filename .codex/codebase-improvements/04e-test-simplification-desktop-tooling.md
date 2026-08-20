# Stage 4 — Test simplification: desktop and tooling (group E)

Scope reviewed: desktop, core CLI, diagnostics, preferences, workspace state,
`parchmint-ci`, packaging/release verification, UI verification, and the
workspace test-support crate. This was source inspection only; no builds or
tests were run. Existing worktree changes were treated as Stage 1–3 context,
not as work to edit.

## Safe simplification (one)

### Fix the embedded-History corruption fixture setup

- Evidence: `crates/parchmint-desktop/tests/production_graph.rs:787-794`,
  `corrupt_embedded_history_does_not_prevent_open_and_reports_typed_recovery_availability`,
  calls `ScopedProject::from_fixture("canonical/minimal-project")`, then
  `fs::create_dir(&git)` before writing `HEAD`.
- Confirmed fixture state: `tests/parchmint-test-support/fixtures/canonical/minimal-project/.git/`
  is checked in and copied by `ScopedProject::from_fixture` through
  `tests/parchmint-test-support/src/lib.rs:249-262,365-377`. Therefore the
  `create_dir` call receives an existing directory and fails with
  `AlreadyExists`; the test never reaches `DesktopBootstrap` or the History
  recovery assertions. This is a brittle test defect, not a production-graph
  regression.
- Replacement: remove only `fs::create_dir(&git)` and retain the existing
  `fs::write(git.join("HEAD"), b"not a valid embedded Git repository\\n")`.
  The copied fixture supplies the managed `.git` directory; corrupting its
  `HEAD` preserves the intended malformed-but-present repository scenario.
- Preserved behavior: production project data must still open; History must be
  reported `Unavailable`; the problem must mention corruption; reinitialize
  availability remains `Blocked`; the UI snapshot remains readable; and
  History listing remains an error (`production_graph.rs:796-823`).

## Candidates examined but intentionally not proposed

- `crates/parchmint-core-cli/tests/native_cli.rs:19-34` and
  `crates/parchmint-core-cli/tests/headless_backend.rs:15-30` have identical
  `Fixture` wrappers around `ScopedProject`. They live in separate integration
  test binaries and exercise different CLI contracts (machine exit/status and
  diagnostics versus recovery/history/search/interchange). Moving the wrapper
  into shared test support would expand public test-support API without
  reducing fixture lifecycle or production-graph evidence; no safe small
  change is justified.
- `crates/parchmint-preferences/tests/native_preferences.rs:16-40` and
  `crates/parchmint-workspace-state/tests/native_workspace_state.rs:14-40`
  each define a temporary-resource guard. Their resource types, cleanup
  semantics (file versus directory), and collision strategies differ. A
  generic helper would obscure those boundaries and is not a simplification
  worth the added shared API.
- The paired desktop close tests added in the Stage 1–3 changes,
  `crates/parchmint-desktop/tests/native_desktop.rs:329-390`, deliberately
  cover clean-close and final-save callback failure separately. They preserve
  distinct retry/stale-request lifecycle evidence and should not be merged.
- Complete-application catalog checks and UI verification exact catalog/hash
  checks are release/UI evidence, not redundant fixture tests; relaxing their
  exactness would lose release-policy or rendered-evidence coverage.

## Result

One high-confidence safe simplification is justified: remove the redundant
`.git` directory creation in the production-graph corruption fixture. No
additional simplification is recommended within this scope.
