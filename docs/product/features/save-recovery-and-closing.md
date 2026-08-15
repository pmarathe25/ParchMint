# Save, recovery, and closing

- **SAVE-001:** Autosave never blocks the UI thread.
- **SAVE-002:** Request autosave 60 seconds after editing becomes idle and at least every 5 minutes during continuous editing.
- **SAVE-003:** Structural changes request immediate asynchronous save/checkpoint.
- **SAVE-004:** Closing a tab, switching projects, or closing a window requests a high-priority save.
- **SAVE-005:** `Save` queues a high-priority save through the current revision but remains nonblocking.
- **SAVE-006:** Status distinguishes dirty, saving, saved-through-revision, and error states.
- **SAVE-007:** A save captures a consistent revision; later edits remain dirty for another save.
- **SAVE-008:** Only dirty canonical resources are serialized and written.
- **SAVE-009:** Canonical writes use crash-safe temporary-write/flush/atomic-replace behavior appropriate to each platform.
- **SAVE-010:** A completed history checkpoint corresponds to successfully written canonical state.
- **SAVE-011:** A high-frequency recovery journal protects changes after the latest completed autosave.
- **SAVE-012:** Recovery data is implementation-specific, versioned, and never the sole copy of completed authored state.
- **SAVE-013:** On save failure, editing remains available, the error persists visibly, recovery remains intact, and the application does not claim Saved.
- **SAVE-014:** A normal close waits asynchronously for final save. Failure keeps the project open with Retry and Cancel Close.
