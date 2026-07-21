# Coding conventions

> Read before changing production code. Load the relevant architecture page for
> ownership and invariants.

Favor explicit ownership, typed failures, deterministic output, and narrow interfaces.

## Rust

- Keep Qt types out of crates below `parchmint-bridge`.
- Put invariants in `parchmint-domain`; orchestration in `parchmint-app`.
- Use stable ID newtypes and typed errors—not unvalidated strings.
- Bound user input before allocation, parsing, or traversal.
- Avoid recursion over user-controlled graph or Markdown depth.
- Preserve deterministic ordering, unknown fields, and opaque source.
- Add a regression test with every behavior fix.

## C++ and bridge

- Use C++20 and Qt ownership; prefer parented `QObject`s and scoped values.
- Adapt Qt documents, cursors, models, and platform APIs—not business rules.
- Expose narrow typed invokables; avoid generic maps where a stable type fits.
- Keep I/O and long work off the UI thread.
- Publish only completions whose generation/revision is still current.

## QML

- Use `DesignTokens`; avoid one-off colors and decorative glyph icons.
- Keep bindings authoritative; key editable buffers by stable node ID.
- Share command IDs across menus, shortcuts, and the command palette.
- Give icon-only controls accessible names and tooltips.
- Provide keyboard parity for drag/drop and every primary action.
- Make destructive consequences and recovery explicit.

## Persistence and async work

- Use storage transaction/atomic-write APIs for canonical replacements.
- Stamp background work with the narrowest relevant revision.
- Recheck freshness immediately before visible commit.
- Surface rollback failure; never report partial recovery as success.
- Update specifications and fixtures with every format change.

Format with `rustfmt`; workspace Clippy `all` and `pedantic` warnings are errors.
See [testing](testing.md) for gates and [documentation conventions](../conventions.md)
for prose changes.
