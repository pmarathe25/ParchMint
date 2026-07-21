# Architecture decision records

> Read the record governing a boundary before changing it. Do not load unrelated ADRs.

ADRs preserve decisions that constrain future changes. Architecture guides
describe the current system; ADRs explain why a durable boundary was chosen.

| ADR | Decision |
|---|---|
| [0001](0001-qt-version-linking-and-license.md) | Qt version, modules, and dynamic linking |
| [0002](0002-qml-text-editor-host.md) | QML `TextArea` hosts the production text document |
| [0003](0003-cxx-qt-boundaries-and-threading.md) | CXX-Qt ownership and asynchronous work |
| [0004](0004-markdown-parser-and-semantic-ast.md) | Source-aware Rust Markdown AST |
| [0005](0005-parchmint-markdown-extensions.md) | ParchMint Markdown extensions |
| [0006](0006-atomic-write-and-recovery-direction.md) | Atomic writes and recovery direction |
| [0007](0007-sqlite-fts-cache.md) | SQLite FTS5 is disposable derived state |
| [0008](0008-testing-and-ci.md) | Testing and cross-platform CI |
| [0009](0009-maintained-yaml-parser.md) | Maintained pure-Rust YAML parser |
| [0010](0010-revisioned-document-lifecycle.md) | Revisioned document lifecycle and recovery |
| [0011](0011-docx-writer-boundary.md) | Contained DOCX package writer |
| [0012](0012-linux-packaging-and-update-strategy.md) | Linux packaging and offline updates |
| [0013](0013-distribution-license.md) | GPL-3.0-or-later distribution license |
| [0014](0014-incremental-transactions-and-revisions.md) | Incremental transactions, revisions, and model deltas |

New records use `NNNN-kebab-case.md` and the sections **Status**, **Context**,
**Decision**, and **Consequences**. Status lives in each record. Never rewrite
an accepted decision to hide history—supersede it.
