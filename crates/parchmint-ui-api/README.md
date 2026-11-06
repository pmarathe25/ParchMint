# `parchmint-ui-api`

**Purpose:** Define framework-neutral UI values and services for one project
session. The desktop assembles implementations; the UI owns presentation state.

## Interface

`ProjectUiPorts` groups query, command, persistence, History, recovery, search,
export, editor, spelling, preference, and workspace services. `access()` validates
the session generation and returns `ProjectUiAccess`; each service access checks
that the session remains current.

`ProjectSnapshotQuery` returns structure, all document summaries, and loaded
bodies. `snapshot_with_documents` loads all live bodies for background operations
such as Export and History. `ProjectWorkflowPort` exposes multi-step application
operations and results.

`ProjectSessionRegistry` issues and retires generation-tagged identities.
`PlatformServices` groups platform interfaces. `apply_appearance_events` delivers
numbered theme snapshots in stable window-ID order. See [lib.rs](src/lib.rs).

The optional assistant workflow port receives a captured request and exact open
editor projections. It returns revisioned evidence and provider output; the UI
must still reject stale session/request results and validate any proposed action.
Explicit pinned and excluded context nodes are captured with each request;
desktop resolves their current descendants and must honor exclusions for notes
and previous conversation as well as project passages. An optional evidence
scope limits Ask; Check always also includes its separate target.
Selection-only requests capture bounded editor text and its revision at send
time. They omit conversation history, and Check retains its separate target.
Optional explicit note access is scoped to one document or the project and
bound to a captured project revision and hierarchy identity. It is separate
from note creation and must honor context limits and exclusions. The scope
permits current note contents at request time, not a frozen note snapshot.
Check results may include locally validated findings and evidence; older saved
results remain readable without them.
The optional progress callback delivers transient, unverified text; only the
returned final result may be persisted or used for citations and actions.

## Session boundary

Recreated sessions receive newer generations. Old ports cannot authorize work
against a replacement even if its logical ID is reused. The UI also checks
request and mount generations when async results arrive.

Service methods expose ParchMint types. Widgets, renderer types, storage-library
values, editor-engine values, and native handles stay in implementation crates.
The desktop owns the startup lifecycle interface.
