# Editor boundary

> Read when changing text input, formatting, source mode, document sessions,
> Markdown serialization, pane state, or editor tests.

Canonical document content is ParchMint Markdown. Rust owns the live body and
its revision; each pane owns only Qt cursor, selection, scroll, focus, and text
undo state.

## Data flow

```text
Qt edit → UTF-16 delta → bridge validation → Rust DocumentSession
                                      ↓
                          journal → canonical save
```

The production pane edits canonical Markdown text. `EditorAdapter` provides
Qt-native cursor, formatting, semantic-object, paste, and undo operations, but
Qt HTML/Markdown output is never written as canonical source. Durable rich
formatting must round-trip through the Rust semantic codec.

## Rules

- One document has one authoritative session across pane navigation and splits.
- Each split owns a stable Qt pane host; closing another split only reindexes its backend binding.
- Split topology and divider ratios are per-project UI state, not canonical project content.
- Deltas use Qt UTF-16 offsets and are rejected on invalid boundaries.
- A rejected delta triggers a full body resync; the editor does not continue from divergent state.
- Source parse errors retain the user buffer until it is fixed or explicitly discarded.
- Unsupported constructs remain opaque source with bounded diagnostics.
- Rich paste keeps allowed semantic runs and removes active/remote content; plain paste stays literal.
- Protected page breaks, images, and opaque objects are explicit semantic units.

## Read next

- Syntax and preservation: [ParchMint Markdown 1](../format/parchmint-markdown-1.md)
- Save lifecycle: [Persistence](persistence.md)
- Revision protocol: [Concurrency](concurrency.md)
- Qt host decision: [ADR-0002](adr/0002-qml-text-editor-host.md)
- Semantic AST decision: [ADR-0004](adr/0004-markdown-parser-and-semantic-ast.md)

Key code: `app/qml/components/PaneHost.qml`, `app/cpp/editor_adapter.*`,
`crates/parchmint-app/src/document.rs`, and `crates/parchmint-markdown`.
