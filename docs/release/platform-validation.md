# Platform validation

> Read when collecting physical release evidence after UI, editor, storage,
> packaging, Qt, or platform changes.

Record OS/architecture, artifact hash, Qt version, display scale, input method,
screen reader, filesystem, and result. Offscreen tests do not replace physical validation.

## Input and accessibility

- Type, commit, cancel, select, undo, and autosave dead keys, CJK IME, Arabic,
  Hebrew, combining marks, Indic text, and emoji sequences.
- Complete primary binder, editing, pane, search, and export workflows by keyboard.
- Verify names, roles, focus order, announcements, visible focus, high contrast,
  reduced motion, and 100/150/200% scaling with Narrator, VoiceOver, or Orca.

## Editor fidelity

- Exercise plain/rich clipboard fixtures; reject active/remote content.
- Verify two panes retain independent selection, scroll, and undo state.
- Navigate and delete protected page breaks, images, and opaque blocks as units.
- Trigger autosave during IME composition; only committed text becomes canonical.
- Introduce invalid source and recover without losing the buffer.

## Installation and lifecycle

- Install, upgrade, associate, uninstall, reinstall, and reopen an existing project.
- Test spaces, Unicode, long paths, removable/network media where supported, and read-only destinations.
- Sleep/resume during save, indexing, compile, and export.
- Terminate during journal, canonical save, backup, transaction, and export commit; verify recovery and intact old destinations.
- Exercise full disk, permission changes, missing assets, and external clean/dirty edits on disposable copies.

Attach results to the [release evidence bundle](process.md).
