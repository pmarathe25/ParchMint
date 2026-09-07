# ParchMint architecture

**Purpose:** ParchMint connects a native writing workspace to local project files
and background services. This page maps the whole system; component READMEs
explain their contracts and implementation. Tests define supported behavior.

## Application flow

The desktop executable constructs services, loads preferences, and starts the
Iced UI. Each open project has one write lease, application session, persistence
coordinator, and window. Reopening that project focuses its existing window.

The UI sends project actions through session-scoped service interfaces. The
application validates those actions and owns project undo. Document input goes
to a shared editor session, which owns document edits and undo. Text fields keep
local undo until they commit a project or document command.

```text
input -> shared editor session changes one document revision
  -> application persistence captures that revision
  -> recovery journal protects unsaved edits
  -> save worker encodes a fixed project snapshot
  -> filesystem writer completes file replacement
  -> History records the matching checkpoint
  -> UI acknowledges the captured revisions as saved
```

**Background work:** File access, Git, SQLite, spellcheck, export, and project-wide
analysis run away from the UI loop. Results carry session, window, document,
query, or mount revisions as appropriate. Callers reject stale results. Edits
made during a save remain dirty; closing waits for the final save and leaves the
window open on failure.

**Service boundaries:** API crates define ParchMint values and operations.
Implementation crates keep Git, SQLite, spelling-engine, editor-engine, Iced,
and OS types private. The desktop supplies concrete adapters. The UI owns
presentation state and calls those adapters through their interfaces.

## Data locations and ownership

| Data | Location and owner |
| --- | --- |
| Current saved project | `project.toml` and its listed resources; project-format defines encoding and project-fs writes files |
| Older saved states | App-managed Git repository at the project root; History adds checkpoints |
| Unsaved recovery records | `.parchmint/recovery/`; recovery-fs persists journal records |
| Rebuildable search index | `.parchmint/cache/search.sqlite`; search-sqlite maintains it |
| Tabs, panes, scroll, and layout | Application-data directory, keyed by project ID; workspace-state persists it |
| Appearance, global dictionary, recent projects | Application preference file; preferences serializes changes |

The manifest includes document word-count summaries. See
[project-format](../../crates/parchmint-project-format/README.md) for the complete
saved resource set and [desktop](../../crates/parchmint-desktop/README.md) for
platform application-data paths.

History contains completed project snapshots. Recovery may contain newer edits.
Search caches, workspace layout, preferences, and recovery records do not enter
History.

| Live state | Owner |
| --- | --- |
| Structure, metadata, styles, deletion tombstones, project undo, global replacement | Application |
| Open document content, comments, anchors, revisions, and document undo | One editor-core session per document |
| Logical cursor and selection | Per-view records in the shared editor-core session |
| Pixel scroll, viewport, focus, layout, search and spelling decorations | Each mounted Iced editor view |
| Temporary fields, menus, tabs, and comment drafts | UI presentation state |

Two panes can share a document session while keeping independent selections and
viewports. Text remains valid UTF-8; editing and spellcheck currently target
normal en-US keyboard input.

## Component map

### Core and persistence

| Component | Contract or responsibility |
| --- | --- |
| [domain](../../crates/parchmint-domain/README.md) | Project values, stable IDs, commands, and validation |
| [application](../../crates/parchmint-application/README.md) | Action routing, project undo, replacement, and persistence coordination |
| [contracts](../../crates/parchmint-contracts/README.md) | Versioned annotation and recovery JSON records |
| [project-format](../../crates/parchmint-project-format/README.md) | Deterministic HTML, TOML, CSS, text, and annotation encoding |
| [project-repository](../../crates/parchmint-project-repository/README.md) | Project open/read, write leases, and multi-file writer contracts |
| [project-fs](../../crates/parchmint-project-fs/README.md) | Validated filesystem access, write locking, and recoverable file replacement |
| [save](../../crates/parchmint-save/README.md) | Serial snapshot writes, matching History checkpoints, and save completion |

### Background services

| Contract | Implementation |
| --- | --- |
| [recovery-api](../../crates/parchmint-recovery-api/README.md): journal and replay | [recovery-fs](../../crates/parchmint-recovery-fs/README.md): files and pending checkpoint records |
| [history-api](../../crates/parchmint-history-api/README.md): complete checkpoints | [history-git2](../../crates/parchmint-history-git2/README.md): embedded libgit2 |
| [search-api](../../crates/parchmint-search-api/README.md): project-wide search | [search-sqlite](../../crates/parchmint-search-sqlite/README.md): rebuildable SQLite FTS5 index |
| [export-api](../../crates/parchmint-export-api/README.md): immutable manuscript plan and sink | [export-html](../../crates/parchmint-export-html/README.md): self-contained HTML |
| [spellcheck-api](../../crates/parchmint-spellcheck-api/README.md): revisioned text checks | [spellcheck-en-us](../../crates/parchmint-spellcheck-en-us/README.md): bundled offline dictionary |

### Desktop and editing

| Component | Responsibility |
| --- | --- |
| [desktop](../../crates/parchmint-desktop/README.md) | Process startup, service assembly, and project-session lifecycle |
| [ui-api](../../crates/parchmint-ui-api/README.md) | Session-scoped services and framework-neutral UI values |
| [ui-iced](../../crates/parchmint-ui-iced/README.md) | Windows, event loop, widgets, and presentation state |
| [editor-api](../../crates/parchmint-editor-api/README.md) | Sessions, views, commands, and exact-revision snapshots |
| [editor-core](../../crates/parchmint-editor-core/README.md) | Framework-independent document sessions and undo |
| [editor-iced](../../crates/parchmint-editor-iced/README.md) | Virtualized Iced editor and mounted-view geometry |
| [platform-api](../../crates/parchmint-platform-api/README.md) | Native dialogs, menus, clipboard, links, paths, and appearance |
| [platform-native](../../crates/parchmint-platform-native/README.md) | Windows, macOS, and Linux implementations |
| [design-system](../../crates/parchmint-design-system/README.md) | Framework-neutral tokens and SVG icons |
| [preferences](../../crates/parchmint-preferences/README.md) | Application settings and process-wide theme changes |
| [workspace-state](../../crates/parchmint-workspace-state/README.md) | Per-project layout persistence |
| [diagnostics](../../crates/parchmint-diagnostics/README.md) | Bounded local logs and optional test observations |

## Verification and distribution

[Shared fixtures](../../tests/parchmint-test-support/README.md) use the production
format codec. The [UI driver](../../tests/parchmint-ui-driver/README.md) exercises
real widgets and services with controlled input, clocks, and completion delivery.
[Visual verification](../../tests/parchmint-ui-verification/README.md) captures and
compares PNGs; [usability review](../../tests/parchmint-ui-driver/USABILITY.md)
checks whether complete tasks are understandable and usable.

[Packaging](../../packaging/README.md) builds native installers and publishes
version-tagged GitHub releases. [Future work](../future-work.md) separates proposed
features from current behavior. Backwards compatibility is not required; add a
migration only for a supported use.
