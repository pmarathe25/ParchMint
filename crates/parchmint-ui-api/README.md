# `parchmint-ui-api`

This crate defines the ParchMint values and session-scoped services that the
native UI uses. It contains no widgets, renderer types, or window handles.
The desktop crate assembles services and owns the startup lifecycle interface.

## Interface

`ProjectUiPorts` groups query, command, persistence, History, recovery, search,
export, editor, spelling, preference, and workspace services for one session.
`access()` checks the session generation and returns `ProjectUiAccess`.
Each service access checks that the session is still current.

`ProjectSnapshotQuery` returns the project structure, summaries for all documents,
and bodies for loaded documents. `snapshot_with_documents` materializes all live
documents for background reads such as Export and History.
`ProjectWorkflowPort` exposes multi-step application operations and their results.

`ProjectSessionRegistry` issues and retires generation-tagged session identities.
`PlatformServices` groups the platform interfaces. `apply_appearance_events`
delivers each numbered appearance snapshot to windows in stable window-ID order.
See [lib.rs](src/lib.rs) for the complete interface.

## Implementation

A recreated session has a newer generation. Old ports cannot authorize commands
against the new session, even when its logical session ID is reused. The native
UI also checks request and mount generations when asynchronous results return.

Service methods expose ParchMint types. The desktop supplies concrete adapters;
the UI converts results to its own presentation state. Library-specific storage,
editor, and native handle types stay inside their implementation crates.
