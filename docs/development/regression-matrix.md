# Version 1 regression matrix

This is the Stage 08 traceability table. “Automated” names the owning test
module; “manual” links the charter used when the behavior requires a real OS,
screen reader, consumer application, or installer. A blocker is explicit and
does not turn an unverified gate into an expected product behavior.

| Master-plan requirement / gate | Coverage | Evidence or blocker |
|---|---|---|
| Human-readable project directory and stable IDs | Automated | Storage create/reopen, example catalog |
| Nested binder groups, reorder, reparent, duplicate, trash/restore | Automated | Domain command/undo tests; binder guide |
| Titles and synopsis-first planning | Automated/manual | Workspace structural tests; platform charter |
| WYSIWYG supported formatting and lossless Markdown | Automated | Markdown matrix fixtures and round-trip tests; Qt editor tests |
| Named paragraph/character styles and inheritance | Automated | Domain style validation; compile style resolution tests |
| Raw source and opaque-content preservation | Automated | `malformed-extensions.md`, raw-buffer lifecycle tests |
| Research notes, attachments, symmetric split panes | Automated/manual | Workspace attachment/pane tests; platform charter |
| Search over manuscript/research metadata and body | Automated | Index FTS and workspace rebuild tests |
| Unicode word/character counts | Automated | Index Unicode rules and Unicode example project |
| Autosave, recovery, rotating backups | Automated | Application document lifecycle tests; recovery fixtures |
| Safe external-file changes | Automated/manual | Dirty/clean conflict tests; platform charter |
| Markdown, text, HTML, PDF, EPUB, DOCX export | Automated/manual | Compile validators and deterministic output tests; consumer blocker below |
| Binder-ordered compile and research exclusion | Automated | Compile selection/preview tests |
| Atomic saves and export collision safety | Automated | Storage atomic-write and compile collision tests |
| Path, symlink, attachment, and hostile-input security | Automated | Storage traversal/symlink tests; Markdown diagnostics |
| No network during normal operation | Review/manual | Dependency inventory and release audit; Stage 09 final audit |
| 10,000 nodes / 10 million words | Automated generator/manual timing | Three corpus manifests and generator tests; Stage 09 performance gate |
| Windows, macOS, Linux build/test/smoke | Partial | Linux evidence in Stage 07; Windows/macOS Stage 09 blocker |
| Keyboard-only primary workflows | Partial | Shortcut reference and Linux Qt smoke; physical platform charter |
| IME, bidirectional text, high-DPI, reduced motion | Partial | Qt synthetic tests and Unicode fixture; physical input Stage 09 blocker |
| Narrator, VoiceOver, Orca accessibility | Manual | Platform charter; Stage 09 blocker until consumer runs |
| Installer, file association, sleep/resume | Manual | Platform charter; Stage 09 blocker |
| Word/LibreOffice/EPUBCheck/browser consumer compatibility | Manual | Export support matrix; Stage 09 blocker |

The source of truth for each row is the current implementation and accepted
Stage 01–07 handoff, not this table. Stage 09 owns only the rows marked as
platform, consumer, release, or performance blockers.
