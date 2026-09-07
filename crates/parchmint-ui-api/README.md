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

## Session boundary

Recreated sessions receive newer generations. Old ports cannot authorize work
against a replacement even if its logical ID is reused. The UI also checks
request and mount generations when async results arrive.

Service methods expose ParchMint types. Widgets, renderer types, storage-library
values, editor-engine values, and native handles stay in implementation crates.
The desktop owns the startup lifecycle interface.
