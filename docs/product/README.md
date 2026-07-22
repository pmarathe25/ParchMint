# Product contract

> Read when changing user-visible scope, supported workflows, platforms,
> privacy posture, or non-goals.

ParchMint is a native, local-first desktop application for planning, writing,
organizing, and exporting long-form work. Projects remain usable as ordinary
Markdown, TOML, and asset files without ParchMint.

## Required capabilities

- Arbitrarily nested manuscript and research structures with stable identity.
- Titles, synopses, labels, status, cards, and deterministic binder order.
- Source-preserving ParchMint Markdown with reusable paragraph and character styles.
- Arbitrarily split editor panes for writing beside research or other documents.
- Search and Unicode-aware statistics from document to project scope.
- Autosave, recovery journals, backups, and explicit external-change handling.
- Binder-ordered Markdown, text, HTML, PDF, EPUB, and DOCX export.
- Keyboard, screen-reader, IME, bidirectional-text, theme, and high-DPI support.
- Bounded use with 10,000 nodes and 10 million words.

## Non-goals

- Browser or mobile clients.
- Cloud accounts, synchronization, collaboration, telemetry, or runtime network access.
- Plugin marketplace or public scripting API.
- Desktop-publishing layout, track changes, citation management, or generative AI.
- Perfect import of arbitrary external Markdown or proprietary document formats.

## Product invariants

- Rust owns canonical state and business rules; Qt owns presentation and platform integration.
- Canonical content is open and portable. `.parchmint/` is disposable local state.
- Unknown format fields and unsupported Markdown remain recoverable source.
- Writes are atomic or journal-backed; stale work cannot overwrite newer revisions.
- Diagnostics remain local unless a user explicitly exports them.
- The application is GPL-3.0-or-later.

Normative details live in the [format specifications](../format/),
[architecture](../architecture/README.md), and [legal documents](../legal/).
