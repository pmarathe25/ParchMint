# S30 — Contracts, Domain, Project Commands, and Canonical Format

## Goal

Freeze durable models, project undo, and format boundaries before adapters/UI spread.

## Tasks

- Stable IDs and project/node/style/metadata/comment entities.
- Ordered-tree invariants and commands.
- `ProjectCommandDispatcher`/`ProjectUndoManager` contracts and bounds/reset behavior.
- Composite global-replacement operation/inverse contract.
- Versioned IPC/application schemas and generated bindings.
- Restricted deterministic HTML codec.
- `project.toml`, annotation JSON, style CSS, and project `dictionary.txt` codecs.
- Migrations.
- Golden/property fixtures for blocks, marks, comments, metadata, tabs, breaks, dictionaries, tombstones.
- Headless create/validate/inspect/roundtrip/project-command/undo commands.
- Adapter-independent title synchronization and word counting.

## Restrictions

No React, DOM, Tauri, ProseMirror, git2, rusqlite, or spellcheck-engine types in public APIs.

## Pass criteria

- Byte-identical canonical round trips.
- Invalid structures rejected.
- Random command/undo/redo sequences preserve invariants.
- Unicode/path tests pass on all three OSes.
- Generated bindings are clean and cross-language fixtures pass.
