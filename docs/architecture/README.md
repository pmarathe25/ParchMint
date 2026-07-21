# Architecture

> Read when deciding where behavior belongs or when a change crosses QML, C++,
> the bridge, and Rust.

ParchMint has one canonical state owner: Rust. Qt presents snapshots and sends
commands; it does not become a second project model.

```text
QML / Qt Quick
      │ commands, IDs, immutable rows, notifications
C++ Qt adapters
      │ QTextDocument operations and Qt item models
CXX-Qt bridge
      │ typed values and revisioned publication
Rust application services
      ├── domain: graph, IDs, commands, invariants
      ├── markdown: source-aware semantic codec
      ├── storage: canonical files and transactions
      ├── index: disposable SQLite projection
      └── compile: immutable IR and exporters
```

## Ownership

| Layer | Owns | Must not own |
|---|---|---|
| QML | Layout, interaction, focus, dialogs, accessibility | Files or domain validation |
| C++ adapters | Qt document/cursor operations, render objects, item models | Project rules or persistence |
| Bridge | Typed Qt API, polling, publication, stale-result rejection | Canonical business logic |
| Application | Workspaces, sessions, commands, workers | Qt types |
| Domain | IDs, graph, styles, presets, invariants | I/O or UI |
| Markdown | Parse, semantic AST, deterministic source preservation | Project storage |
| Storage | Paths, manifests, locks, atomic writes, recovery | Presentation |
| Index | Rebuildable FTS and cached counts | Authoritative content |
| Compile | Binder traversal, IR, validation, exporters | Live mutable state |

## Load the narrow guide

- Editing or Markdown: [Editor boundary](editor.md)
- Files, saves, recovery, external changes: [Persistence](persistence.md)
- Workers, revisions, cancellation: [Concurrency](concurrency.md)
- Domain, search, statistics, compile, export: [Application services](services.md)
- Durable rationale: [ADR index](adr/README.md)

## Hard boundaries

- QML never opens a project path.
- Qt types stop at `parchmint-bridge`.
- SQLite and `.parchmint/` are derived/local state.
- Qt Markdown or HTML conversion is never canonical serialization.
- User-controlled trees and documents are bounded and traversed without unsafe recursion.
- Async results become visible only after a final generation/revision check.
