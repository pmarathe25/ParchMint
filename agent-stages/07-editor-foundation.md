# S60 — ProseMirror Editor Foundation

## Goal

Implement the highest-risk editor adapter and run the early native runtime gate before broad feature integration.

## Tasks

- Implement the v1 ProseMirror schema, stable block/style IDs, marks, lists, quotations, atomic Scene/Page Breaks, and literal tabs.
- Implement deterministic canonical HTML parse/serialize adapters and paste sanitization.
- Implement `SharedEditorSession` with two independent view sessions and shared document history.
- Implement one always-visible shared toolbar with focused-view routing, plus view attach/detach/restore, selection/scroll/search state, and transaction mapping.
- Implement worker projection, changed-block/title/word-count output, recovery batching, and revision acknowledgements.
- Implement foundational comment anchors/decorations, focused-view anchor geometry, editor-context-menu comment creation, and Comments-panel commands sufficient for the runtime gate. Do not add a floating selection-end affordance.
- Run release-mode native runtime checks on Windows WebView2, macOS WKWebView, and Linux WebKitGTK using ordinary and approximately 250,000-word documents with the same visible functionality.

## Restrictions

- Do not add a size-based feature mode.
- Do not disable second-view, comments, formatting, or search behavior based on size.
- Do not fork ProseMirror/Tauri/webviews broadly.
- Do not alter canonical formats to match ProseMirror internals.

## Pass criteria

The architecture and acceptance-plan editor/runtime gates pass, including canonical fidelity, two-view correctness, typing responsiveness, IME/clipboard, accessibility, memory stability, and cross-platform launch behavior.

If the selected stack cannot meet a mandatory requirement after bounded implementation optimization, stop at G20 with raw evidence. Do not choose another frontend.
