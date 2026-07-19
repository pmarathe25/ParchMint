# ADR-0002: QML TextArea hosts the production text document

Status: Accepted (Stage 01)

## Decision

Use Qt Quick `TextArea`/`QQuickTextDocument` with a C++ `EditorAdapter` operating
through `QTextCursor`, `QTextBlockFormat`, and `QTextCharFormat`. Keep editor
semantics in explicit Qt format properties and custom object types. Each editor
owns a separate `QTextDocument` and therefore separate cursor, selection, scroll,
and text undo state.

## Rejected alternative

A hosted Widgets `QTextEdit` would provide mature desktop editing behavior, but
embedding it into Qt Quick adds a second scene/focus/accessibility stack,
platform composition risk, and awkward split-pane integration. The required
document primitives are available without hosting a widget.

## Measured fallback

If a target platform exposes a blocker that cannot be fixed in the Quick host,
the semantic adapter remains `QTextDocument`-based and can drive an isolated
Widgets harness. Switching the production host requires measured IME,
accessibility, focus, and performance evidence plus an ADR.
