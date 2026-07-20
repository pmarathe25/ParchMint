# ADR-0010: revisioned document lifecycle and recovery records

Status: Accepted (Stage 03)

## Decision

Keep one `DocumentSession` per open document in the Rust application layer.
Qt emits revisioned dirty block ranges and requests commands; it does not
serialize the full document per keystroke. Serialization, journal replacement,
backup creation, and canonical `ProjectStorage::save_document` execute on one
serial worker per open project. Full structural saves remain explicit project
operations; an editor autosave never rewrites unrelated canonical documents.

A journal must durably replace its prior record before the matching canonical
save may be scheduled. Every request and completion carries `WorkStamp`
(project generation plus document revision). Stale completions are ignored and
stale requests are rejected immediately before persistence. Canonical/external
fingerprints compare byte length and fixed FNV-1a; they detect changes but are
not an authenticity mechanism.

## Consequences

Focus loss forces the normal journal path. A successful canonical replacement
is the only transition to “Saved.” Recovery records contain a complete Markdown
body so they remain independently previewable after abrupt termination.
External content auto-reloads only when the session is clean; dirty sessions
require explicit reload, overwrite, or save-copy resolution. Source-mode and
external resolution each start a new text-undo epoch.
