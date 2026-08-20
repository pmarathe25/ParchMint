# Stage 6 DRY audit — UI and platform

Scope reviewed: production code in `parchmint-ui-api`, `parchmint-ui-iced`,
`parchmint-editor-iced`, `parchmint-design-system`, `parchmint-platform-api`,
and `parchmint-platform-native`. Source inspection only; no builds, tests, or
metadata scans were run. Three high-confidence findings are listed.

## Findings

### 1. Project-session authorization failure construction is duplicated (high confidence)

Evidence: `ProjectUiPorts::access` at
`crates/parchmint-ui-api/src/lib.rs:667-674` and
`ProjectUiAccess::services` at `:683-690` each call `is_current`, then return
the same `StaleProjectSession { session: self.session }` shape on failure.
The second check is intentional: every service operation must revalidate the
session after an access handle is acquired, so this should not be removed or
cached.

Risk: the authority boundary or stale-session error payload can drift between
initial access and per-operation service acquisition, weakening the current
worker/authority invariant.

Smallest solution: add a private `ProjectUiPorts::authorize()` (or equivalent
private helper taking the capability) that performs the current-generation
check and constructs `StaleProjectSession`; call it from both sites. Preserve
the separate checks and all existing borrowed-service lifetimes.

Validation: retain the existing stale-session checks and add/keep focused
coverage for both an access-time stale session and a session becoming stale
before `ProjectUiAccess::services` is used.

### 2. Stable 16-byte ID hex formatting is copied across UI-iced modules (high confidence)

Evidence: `editor_workspace::stable_id_string` at
`crates/parchmint-ui-iced/src/editor_workspace.rs:2721-2729` and
`project_runtime::stable_id_string` at
`crates/parchmint-ui-iced/src/project_runtime.rs:1868-1876` are byte-for-byte
identical: both allocate 32 characters and format every byte as lowercase
two-digit hexadecimal.

Risk: the UI's serialized stable-ID representation can diverge if one parser
or producer changes formatting (case, width, or ordering) independently; the
runtime already parses these IDs and the workspace produces them.

Smallest solution: place one `pub(crate)` `stable_id_string` helper in the
crate root or a small existing UI-iced helper module, and have both modules
call it. Keep `parse_stable_hex` and ID-kind-specific validation in
`project_runtime`; only the shared serialization is consolidated.

Validation: existing stable-ID round-trip and resolver tests should assert the
same lowercase 32-character output from both call paths.

### 3. Linux native-menu handle compatibility is repeated for attach and detach (high confidence)

Evidence: `parchmint-platform-native/src/native_menu.rs` repeats the exact
Xlib/Xcb/Wayland window/display pairing match in `attach` at lines `33-41` and
`detach` at lines `51-57`. Both return the same failure text for mismatches;
only the success value differs (`AttachmentKind::InWindow` versus `Ok(())`).

Risk: adding support for a platform handle pair to one lifecycle operation but
not the other leaves native-menu cleanup asymmetric and can strand an
attachment.

Smallest solution: add a private Linux-only `matching_handles(window, display)
-> bool` predicate and use it in both functions, retaining each function's
distinct success result and existing error construction.

Validation: keep focused attach/detach tests for each supported matching pair
and for mismatched handles; verify both operations reject the same matrix.

No additional high-confidence findings

The design-system token/icon parsing, platform API validation and capability
types, editor layout/rendering, native backend command wrappers, and shared UI
styles either have distinct semantics or already centralize their behavior.
The duplicated native command output paths intentionally differ in trimming
(`output_text`) versus preserving raw output (`command_output_raw`), so they
should not be merged.
