# Stage 02: project domain and storage

Difficulty: hard  
Recommended model: GPT-5.6 Terra, high or xhigh reasoning  
Escalate to Sol when: format migration, filesystem durability, or graph invariants remain unresolved  
Depends on: stage 1  
Master references: `PLAN.md` “Domain and dependency contract,” “Content and project-format contract,” and “Data-integrity, security, and privacy contract”

## Outcome

Implement ParchMint's canonical, open, versioned project model and all non-editor persistence. At completion, projects can be created, validated, opened, structurally edited through domain commands, closed, migrated, inspected outside ParchMint, and rebuilt without `.parchmint/`.

## Primary ownership

- `crates/parchmint-domain`
- `crates/parchmint-storage`
- Format-facing portions of `crates/parchmint-markdown`
- `docs/format`
- Project and migration fixtures
- Minimal bridge commands needed to exercise project open/create/close

## Required work

### 1. Domain types and invariants

- Implement strongly typed project, node, document, style, asset, and revision IDs.
- Implement `Project`, `Node`, `NodeKind`, metadata, style definitions, workspace references, and compile-preset placeholders.
- Enforce rooted acyclic hierarchy, unique membership, valid parents, deterministic sibling order, safe relative paths, and acyclic style inheritance.
- Define built-in manuscript/research roots and built-in styles without hard-coding their display strings throughout the app.
- Define validated commands and emitted events for create, rename, reorder, reparent, duplicate, trash, restore, metadata edit, and style-definition mutation.
- Implement structural undo payloads without coupling them to Qt undo.

### 2. Canonical schemas

- Finalize and document version 1 schemas for `parchmint.toml`, `outline.toml`, `styles.toml`, document front matter, and trash tombstones.
- Use stable UUID file names and project-relative paths.
- Keep titles and summaries in document front matter, not duplicated in the outline.
- Preserve unknown keys where forward compatibility permits it.
- Define deterministic field and collection serialization so repeated saves produce clean diffs.
- Provide a JSON Schema-like or equivalent machine-readable validation artifact where practical, even though the canonical files are TOML/YAML/Markdown.

### 3. Storage services

- Implement create, open, validate, save, close, and reopen.
- Implement atomic same-directory replacement, flush behavior, and descriptive errors.
- Implement a project lock/advisory-open strategy that does not prevent legitimate read-only access or recovery.
- Resolve all project paths defensively; reject traversal and symlink escape.
- Set practical size and nesting limits for manifest/front-matter parsing while allowing the stress target.
- Keep `.parchmint/workspace.toml` optional and `.parchmint/index.sqlite` entirely disposable.
- Store trashed user documents under canonical `trash/`, never only under `.parchmint/`.

### 4. Migrations and compatibility

- Implement format-version detection and a migration pipeline starting with a no-op v1 baseline.
- Create a pre-migration backup before the first canonical mutation.
- Make migrations idempotent where possible and test interruption/retry behavior.
- Fail safely on newer unsupported versions and provide actionable diagnostics.
- Add a read-only validation/diagnostics command suitable for support and CI.

### 5. Tests and examples

- Add domain property tests that generate reorder, reparent, trash, and restore sequences while continuously checking invariants.
- Add golden schema and deterministic serialization tests.
- Add malformed, hostile-path, unknown-key, newer-version, and interrupted-migration fixtures.
- Add a small hand-authored example project that remains understandable in a normal text editor.
- Verify that removing `.parchmint/` leaves a complete, openable project.

## Acceptance gate

- All structural command sequences preserve domain invariants under property testing.
- Canonical files are deterministic, documented, and human-readable.
- Project create/open/edit/close/reopen tests pass using real temporary directories.
- Atomic-write and migration failure injection never corrupts the last acknowledged canonical state.
- Path traversal and symlink-escape cases are rejected.
- Removing `.parchmint/` loses no manuscript, research, active metadata, styles, assets, or recoverable trash.
- The stage's public Rust APIs contain no Qt types.

## Out of scope

- WYSIWYG body editing and document autosave
- Binder/outline/card production UI
- Full-text indexing
- Export implementation

## Handoff

Create `docs/handoffs/02-project-domain-and-storage.md`. Include format version, schema paths, public command/event APIs, migration guarantees, filesystem assumptions, and any cases stage 3 must preserve during editor saves.
