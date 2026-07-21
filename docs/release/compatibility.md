# Format 1 compatibility promise

> Read when changing canonical schemas, migrations, unknown-field handling, or
> version support.

The canonical project format, ParchMint Markdown extensions, and recovery record
format documented under `docs/format/` are frozen as version 1. Version numbers
for project and recovery schemas are independent from the application version.

ParchMint 1.x will:

- open every valid format-1 project without requiring `.parchmint/`, SQLite, or
  data produced only by the installed application;
- preserve unknown front-matter keys and source-backed opaque Markdown unless a
  user explicitly edits or converts that content;
- never silently open a newer canonical schema for writing;
- create a recoverable backup before any future canonical migration;
- keep canonical active data and project trash in documented Markdown/TOML and
  assets, with stable IDs and deterministic binder order;
- retain the documented format-1 read path for the lifetime of application 1.x.

A future format version may add optional fields that version 1 safely preserves.
Any incompatible change requires a new schema number, migration documentation,
golden fixtures, and an accepted ADR. Export file formats are interoperability
outputs, not canonical project storage, and retain the separate exporter support
matrix.
