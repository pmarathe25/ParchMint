# S30 — Contracts, Domain, and Canonical Format

## Goal

Freeze the durable model and format boundaries before adapters and UI spread.

## Tasks

Implement Phase 1 of the implementation plan:

- Stable IDs and project/node/style/metadata/comment entities.
- Strict group/document hierarchy and ordered-tree commands.
- Versioned IPC/application schemas and generated bindings.
- Restricted deterministic HTML schema and codec.
- `project.toml`, annotation JSON, and style CSS codecs.
- Migrations.
- Golden fixtures for all v1 blocks, marks, comments, metadata, literal tabs, and structural breaks.
- Headless `create`, `validate`, `inspect`, and `roundtrip` commands.
- Adapter-independent title synchronization and word counting.

## Ownership rules

Do not import React, DOM, Tauri, ProseMirror, `git2`, or `rusqlite` types into public domain/format APIs.

## Required outputs

- Domain and project-format crates/packages.
- Versioned schemas and generated bindings.
- Golden fixtures and property tests.
- Contract/version manifest for dependent stages.

## Pass criteria

- Byte-identical canonical round trips.
- Invalid structures rejected.
- Path/Unicode fixtures pass on Windows, macOS, and Linux.
- Dependency-boundary checks pass.
