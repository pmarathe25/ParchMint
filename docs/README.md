# Documentation router

> Read this first when a task crosses an unfamiliar area. Load one route, then
> follow only links required by the change.

ParchMint documentation is organized by decision type—not by implementation
history. Avoid reading the whole tree.

| Task | Read first | Load next only if needed |
|---|---|---|
| Product behavior or scope | [Product contract](product/README.md) | Relevant [user workflow](user-guide/README.md) |
| QML, Qt, bridge, or editor | [Architecture](architecture/README.md) | [Editor boundary](architecture/editor.md), [conventions](development/conventions.md), [shortcuts](reference/keyboard-shortcuts.md) |
| Project graph, files, saves, recovery | [Persistence](architecture/persistence.md) | [Project format](format/project-format-1.md), [recovery format](format/recovery-1.md) |
| Markdown parsing or serialization | [Editor boundary](architecture/editor.md) | [Markdown format](format/parchmint-markdown-1.md), relevant [ADR](architecture/adr/README.md) |
| Search, statistics, compile, export | [Application services](architecture/services.md) | [Export fidelity](reference/export-fidelity.md), [performance](development/performance.md) |
| Workers, revisions, stale results | [Concurrency](architecture/concurrency.md) | ADRs [0003](architecture/adr/0003-cxx-qt-boundaries-and-threading.md), [0010](architecture/adr/0010-revisioned-document-lifecycle.md), [0014](architecture/adr/0014-incremental-transactions-and-revisions.md) |
| Build, test, or contribute | [Developer guide](development/README.md) | [Setup](development/setup.md), [testing](development/testing.md) |
| Packaging or release | [Release process](release/process.md) | [Platforms](release/platforms.md), [validation](release/platform-validation.md), [legal](legal/) |
| Documentation | [Documentation conventions](conventions.md) | This router and the affected source-of-truth page |
| Architecture decision | [ADR index](architecture/adr/README.md) | Closest current-state architecture page |
| Planned multi-step work | [Plan format](../plans/README.md) | Only the designated plan |

## Authority order

When documents disagree, use this order:

1. Tests and code for current behavior.
2. Format specifications and accepted ADRs for compatibility constraints.
3. Product, architecture, user, development, and release guides.
4. Plans for intended work; plans never redefine current behavior.

Fix the stale document in the same change. See the
[documentation conventions](conventions.md) before adding a file.
