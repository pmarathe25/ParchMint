# Platform test charter

Run this manual charter on Windows, macOS, and Linux. Record OS version,
architecture, Qt version, display scale, input method, screen reader, and
installer build in the handoff. A synthetic offscreen pass is useful evidence
but does not replace physical platform validation.

## Input and accessibility

- Type dead keys, CJK/IME pre-edit text, Arabic, Hebrew, combining marks, and
  emoji; commit, cancel, select, undo, and autosave each sequence.
- Complete binder navigation, editor formatting, pane switching, search, and
  export using only the keyboard.
- Run with Orca, VoiceOver, and Narrator where available. Check names, roles,
  focus order, announcements, and visible focus at high contrast.
- Repeat at 100%, 150%, and 200% scale and with reduced motion.

## Installation and files

- Install, upgrade, uninstall, and reopen a project after reinstall.
- Check file associations for `.md` and project directories without allowing
  the installer to modify a project.
- Open from paths containing spaces, Unicode, long names, and network/removable
  media where supported.

## Lifecycle and failure

- Sleep/resume during autosave, indexing, compile, and export.
- Modify clean and dirty Markdown files externally; confirm reload versus
  conflict choices and no silent overwrite.
- Terminate during journal, canonical save, backup, and export replacement;
  restart and verify recovery, intact old destinations, and visible warnings.
- Exercise missing assets, duplicate names, read-only destinations, full-disk
  simulation, and permission changes using disposable copies only.

The remaining physical Windows/macOS consumer checks, installer validation,
and screen-reader evidence are explicit Stage 09 work, not hidden assumptions.
