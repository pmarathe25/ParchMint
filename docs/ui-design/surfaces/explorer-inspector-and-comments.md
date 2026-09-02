# Explorer, Inspector, and comments

- Create comments from the editor context menu. An attached range is an actual
  highlight; hovering it opens a compact, interactive thread card beside that
  text. The card is anchored to the range geometry rather than the pointer, so
  it does not jump while the cursor moves. It remains available while moving
  from the anchor into the card, then dismisses after the pointer leaves both.
  New-comment drafting, replies, edits, status changes, and deletion all live
  in this card and do not change editor focus.
- Tree rows are compact and non-overlapping, with no checkboxes or
  active-document rail. Use stronger treatment for focused-pane selection and
  quieter treatment for another pane's active document.
- When Explorer is visible, show Manuscript and Research as independent
  collapsible sections, even when Research has no open document. Exceptions:
  Global Search replaces Explorer, or Explorer is intentionally hidden or
  collapsed. Global Search is at the Explorer header's right edge. Explorer
  context menus expose only applicable create, open, companion, rename, copy,
  cut, and delete actions.
- Inspector has collapsible Synopsis, Metadata, and Comments for documents;
  group Inspector omits Comments. Synopsis and metadata use separate outlined
  editable fields. Metadata order follows Settings.
- Comments are one continuous, read-only document index. Each compact row
  shows its root comment, anchor summary, and textual status; clicking it
  reveals the relevant text in the editor without stealing focus. Its section
  header shows plain secondary `· N unresolved` when any thread is unresolved,
  otherwise `· N comments`. Thread controls never appear in Inspector.
