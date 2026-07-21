# Architecture navigation

ParchMint keeps canonical content in ordinary project files. Rust owns the
domain graph, Markdown codec, storage, search, compile IR, and exporters; Qt
owns windowing, QML, platform adapters, and the `QTextDocument` editor host.
CXX-Qt is the narrow bridge between them. QML never opens project files.

Start with these documents when changing an area:

| Area | First read | Stable boundary |
|---|---|---|
| Product and quality gates | [`PLAN.md`](../../PLAN.md) | Cross-stage contract |
| Project graph and files | [`project-format-1.md`](../format/project-format-1.md) | TOML/YAML/Markdown |
| Markdown semantics | [`parchmint-markdown-1.md`](../format/parchmint-markdown-1.md) | Source-aware AST |
| Recovery | [`recovery-1.md`](../format/recovery-1.md) | Revisioned journal |
| Editor bridge | [`ADR-0002`](../architecture/0002-qml-text-editor-host.md) | Qt adapter |
| Threading | [`ADR-0003`](../architecture/0003-cxx-qt-boundaries-and-threading.md) | Stamped workers |
| Compile/export | [`Stage 07 handoff`](../handoffs/07-compile-and-export.md) | Immutable compile IR |
| Incremental scale | [`ADR-0014`](../architecture/0014-incremental-transactions-and-revisions.md) | Dirty sets, revisions, model deltas |
| Performance gates | [`performance-budgets.md`](performance-budgets.md) | Reference corpus budgets |

The stage index and handoffs explain why a boundary exists. If an edit would
change a persisted schema, ownership rule, recovery guarantee, or public API,
stop and propose an ADR before implementing it.
