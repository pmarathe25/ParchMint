# Stage 6 DRY audit — services and contracts

Scope reviewed: `parchmint-search-sqlite`, `parchmint-search-api`,
`parchmint-spellcheck-en-us`, `parchmint-export-html` (and export API),
`parchmint-contracts`, `parchmint-preferences`, `parchmint-workspace-state`,
and `parchmint-diagnostics` production sources. Source inspection only; no
builds, tests, or metadata scans were run.

## Findings

### 1. Preferences and workspace state duplicate the durable replace protocol (high confidence)

Confirmed parallel implementations:

- `FilePreferenceStore::replace_durably` in
  `crates/parchmint-preferences/src/lib.rs:330-353` creates a unique temporary,
  writes bytes, `sync_all`s the file, renames it over the destination, syncs the
  parent directory, and removes the temporary on failure.
- `FileWorkspaceStateStore::replace_durably` in
  `crates/parchmint-workspace-state/src/lib.rs:378-400` repeats the same
  protocol and failure cleanup, with only path/error-construction differences.
- The same platform-specific directory-sync helper is also duplicated at
  `preferences/src/lib.rs:755-765` and `workspace-state/src/lib.rs:583-591`.

Why this matters: a future durability or cleanup fix can land in one store and
not the other, despite both being application-owned JSON state. The temporary
name allocation is intentionally store-specific, so the shared seam should be
the byte-write/flush/rename/cleanup protocol rather than a broad filesystem
utility.

Smallest shared solution: introduce one narrowly scoped `atomic_replace_bytes`
helper in an already-common persistence/filesystem crate (or a shared module
already owned by these stores), returning `io::Result<()>`; let each store keep
its existing temporary-path policy and map the returned operation errors into
its own `PreferenceError`/`WorkspaceError`. Put the platform-specific
`sync_directory` implementation behind that same seam. Preserve the current
create-new temporary, file sync, directory sync, and best-effort cleanup
ordering.

Validation: add focused failure-path tests at the two store boundaries (rename
or directory-sync error leaves no temporary) and a success test that reloads
the replacement. Existing store tests should continue to cover revision and
path-specific behavior; no broad test consolidation is needed.

### 2. Contract fixture validation repeats the same typed decode/check/re-encode arm (high confidence)

`parchmint-contracts::validate_fixture` at
`crates/parchmint-contracts/src/lib.rs:145-160` has three arms for annotation
sidecar, recovery record, and CLI output. Every arm performs exactly the same
three operations—`serde_json::from_slice`, `validate_schema_name`, and
`serde_json::to_vec`—with only the generated binding type changing.

Why this matters: adding or changing a contract can accidentally omit schema
name validation or canonical re-encoding in one arm, causing contract checks
to diverge silently.

Smallest shared solution: use a local macro (or a private typed helper with a
`Deserialize + Serialize` bound and a schema accessor) to encode the common
decode/check/re-encode sequence, passing only the generated type at each arm.
Keep the descriptor dispatch and unknown-contract fallback unchanged; this is
not a request for a generic contract framework.

Validation: retain the existing malformed, wrong-schema, and fixture tests and
add one table-driven assertion that each registered typed descriptor executes
the shared path.

## No additional high-confidence findings

Search/index projection and query logic, spellcheck request/check paths,
HTML chunking/escaping, workspace pruning, and diagnostics event/timing code
showed distinct responsibilities or deliberately local helpers. No further
duplication met the threshold for a small shared solution without inventing a
broad utility or extracting one-use code.
