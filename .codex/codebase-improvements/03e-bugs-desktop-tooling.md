# Desktop/tooling bug audit

## Confirmed: close-notification failure unregisters a still-live project

Evidence: `crates/parchmint-desktop/src/lib.rs:721-735` removes the project with
`unregister(project, session)` before calling `ui.project_closed(window)`. The
successful-final-save path at `:752-758` has the same ordering.

Trigger: make the UI's `project_closed` callback return an error (for example,
the native window teardown callback fails) during either `close_clean_project`
or `resolve_final_save(request, Ok(()))`.

Impact: the API returns `DesktopError::Ui`, but the writable session lease has
already been dropped and the runtime no longer tracks the project/window. A
retry returns `MissingProject`; reopening can create a replacement window while
the original UI still exists. This also makes a failed close look partially
successful to the lifecycle coordinator.

Minimal fix: call `project_closed` first and unregister only after it succeeds,
or retain an explicit close-notification-pending state that permits retry.

Focused regression tests:

- For `close_clean_project`, inject a UI callback that fails once. Assert the
  first call returns `DesktopError::Ui` and `is_current_window(window)` remains
  true; retry and assert the callback succeeds and the project is then
  unregistered.
- For `resolve_final_save(Ok(()))`, use the same one-shot callback failure and
  assertions. Also verify a stale final-save result remains ignored after a
  genuinely completed close.

## Uncertain observation: architecture CI scans only `crates/`

Evidence: `tools/parchmint-ci/src/main.rs:124-160` reads immediate children of
`crates/` and applies the reviewed package/dependency policy to those manifests.
It does not enumerate all workspace members from the root manifest. A new
workspace member outside `crates/` could therefore evade this check.

This is uncertain because the policy may intentionally cover only product
crates. Confirm the intended scope before changing it. If all workspace members
are in scope, derive the member list from the root Cargo manifest and add a
fixture covering an out-of-`crates/` member.

Evidence limits: source and existing-test inspection only; no builds, tests,
Cargo metadata, or heavy scans were run. No production files were changed.
