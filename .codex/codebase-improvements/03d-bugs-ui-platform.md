# Stage 3 UI/platform bug audit (group 4)

## Confirmed bug: window-scoped native work runs after the capability is closed

**Evidence:** `crates/parchmint-platform-native/src/lib.rs`,
`NativeServices::spawn_window` (around lines 205–224), calls
`registry.authorize(window)` once, then starts `async_task::dispatch` with a
worker closure. That closure immediately calls `work(backend)` and only then
calls `registry.complete`, whose authorization converts the result to
`PlatformError::StaleCapability` if the window was closed meanwhile.

**Trigger/reasoning:** call `DialogService::choose_path`,
`ClipboardService::read/write`, or `ExternalOpenService::open` for a registered
window; close/unregister that capability before the spawned worker gets to its
closure (the existing `testing::BlockingBackend` can deterministically hold
the worker at this point). The operation still executes against the OS: a file
dialog can appear after its window is gone, a clipboard write can happen, or a
validated URL can be opened. Only the completion is rejected as stale, so the
caller sees an error while the side effect has already occurred. This violates
the API/native contract comments requiring authorization immediately before
work starts and makes close/recreate races user-visible.

**Minimal fix:** re-check `registry.authorize(window)` inside the dispatched
worker immediately before invoking `work(backend)` (and deliver that stale
error through the same completion path). Keep the completion-time check as the
second guard.

**Focused regression test:** add a platform-native test using the existing
blocking fake backend: register capability A, start `choose_path(A, ...)`, hold
the worker before backend invocation, unregister A, release the worker, and
assert the fake backend invocation count remains zero and the future resolves
to `StaleCapability`. Repeat for `open` or clipboard write to cover non-dialog
side effects.

## Uncertain observation (not counted as a confirmed bug)

`NativeServices::install` assigns monotonically increasing menu bindings, but
the backend `SystemBackend::install_menu` stores its semantic snapshot by
window only. Two concurrent installs can execute backend work out of order,
leaving that backend map with an older menu even though `installed_menus`
correctly rejects the older completion. The current native attachment path
reads `installed_menus`, so this may be test-only/stale bookkeeping rather than
a visible menu defect; it needs a targeted concurrent-install test before
reporting as a product bug.
