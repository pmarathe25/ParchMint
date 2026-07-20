# ParchMint project format 1

Format version 1 is an ordinary UTF-8 directory. `parchmint.toml`,
`outline.toml`, `styles.toml`, Markdown documents, assets, and `trash/` are
canonical. `.parchmint/` is disposable local state.

## Root files

`parchmint.toml` has `format_version = 1`, `project_id` (UUID), and `name`.
Unknown root keys are retained on an open/save cycle. Newer versions are never
rewritten by an older application.

`outline.toml` has `format_version = 1`, `roots` (manuscript then research),
and `nodes`. Each node has its UUID `id`, tagged `kind`, optional `parent`, and
an explicitly ordered `children` array. It intentionally has no title or
summary. `trash` contains tombstones with `node_id`, prior `parent`, and prior
sibling `index`.

`styles.toml` has `format_version = 1`, ordered `styles`, and optional
`compile_presets`. Style IDs are UUIDs. Built-ins carry immutable machine keys;
display names are not references. `kind` is `paragraph` (the backward-compatible
default) or `character`; inheritance and replacement stay within one kind.
`based_on` must form no cycle. Paragraph-only `next_style` selects the style for
a newly created following paragraph. `properties` is a bounded string map that
exporters resolve through inheritance without Qt.

Required built-ins have stable machine keys/IDs: Body
`018f0be2-a8ea-7d2d-89ea-45aa663708d4`, Heading
`018f0be2-a8ea-7d2d-89ea-45aa663708d5`, and character Emphasis
`018f0be2-a8ea-7d2d-89ea-45aa663708d6`. A loader supplies a missing required
built-in in memory so early format-1 projects remain openable.

Collections are written in UUID order except `roots` and node `children`, whose
order is semantic. Fields are emitted in declaration order. TOML unknown keys
are preserved where they occur at a file's top level.

`assets.toml` is an optional, independently versioned attachment catalog. Its
current `version` is 1 and its ordered `attachments` entries contain an asset
UUID, display name, UUID-derived safe filename, conservative media type, and
byte count. The display name is metadata only; imports never use it as a path.
Attachment bytes live at `assets/<asset-id>.<safe-extension>`. Missing,
symlinked, escaped, oversized, duplicate-ID, or size-mismatched entries make
the catalog invalid. An absent catalog means the project has no attachments.

## Documents and trash

An active document lives at `manuscript/<node-id>.md` or
`research/<node-id>.md`. A trashed document lives at `trash/<node-id>.md`, and
its tombstone also lives at `trash/<trashed-root-node-id>.toml`. UUID file names
mean a rename never changes a body file name.

Every document starts with a YAML mapping at byte zero:

```markdown
---
document_id: 018f0be2-a8ea-7d2d-89ea-45aa663708d4
title: Chapter One
summary: A storm arrives.
labels:
  - draft
future-plugin: retained
---
Body in ParchMint Markdown 1.0.
```

`document_id`, `title`, `summary`, `status`, `labels`, `tags`, `flags`, and
optional `attachment` (an asset UUID) are
known keys. Other string-keyed mappings are retained. Titles and summaries are
only here, never copied to `outline.toml`. YAML is limited to 256 KiB and 64
levels; a whole document is limited to 64 MiB and a TOML manifest to 4 MiB.

All paths are portable relative paths with normal components only. Storage
rejects absolute paths, traversal, and any symlink in a resolved project path.

## Migration and durability

Version 1 opens through a no-op migration step. A future migration must first
copy all canonical files to `.parchmint/backups/pre-migration-v<old-version>`;
the copy is idempotent. Canonical writes use a same-directory temporary file,
file flush, atomic replacement, then Unix directory flush. A project may be
opened read-only while a writer advisory lock exists.

The corresponding machine-readable artifact is
[`project-format-1.json`](project-format-1.json). It is intentionally
JSON-Schema-like: TOML/YAML syntax and cross-file graph validation remain owned
by the Rust validator.
