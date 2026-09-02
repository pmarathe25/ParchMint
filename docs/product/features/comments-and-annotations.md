# Comments and annotations

- **CMT-001:** Documents support range, cursor-position, and document-level comments.
- **CMT-002:** A comment is a thread with a root message and chronological replies. Every thread visibly distinguishes unresolved and resolved state without relying on color alone.
- **CMT-003:** The editor-anchored thread card provides an in-thread reply composer. Users add replies, edit/delete messages, resolve, reopen, and delete threads there. Threads appear in one Inspector index without separate resolved/unresolved sections.
- **CMT-004:** Comment bodies are plain text with paragraph breaks.
- **CMT-005:** The active document's Inspector always includes a read-only Comments index or an explicit empty state. It lists every attached thread with its root message, anchor summary, and status; it never provides a draft or thread-mutation controls.
- **CMT-006:** Selecting an Inspector comment scrolls to and highlights its anchor in the last-focused view of that document without taking editor focus.
- **CMT-007:** The editor context menu provides Add Comment for the current selection or cursor. Selecting text does not add a floating affordance.
- **CMT-008:** With no selection, Add Comment creates a position comment. A selected range opens an anchored editor composer for a range comment.
- **CMT-009:** Comments are stored in JSON sidecars outside canonical HTML prose.
- **CMT-010:** Text anchors include stable block ID, range, quotation, and context sufficient for conservative reattachment.
- **CMT-011:** Editor changes map anchors. If recovery or transformation is ambiguous, the comment becomes orphaned and must not attach to uncertain text.
- **CMT-012:** Orphaned comments remain visible and can be reattached or converted to document-level.
- **CMT-013:** Comments are not copied when a document is duplicated.
- **CMT-014:** Comments are excluded from export and v1 global search.
- **CMT-015:** Hovering an attached range or position anchor shows an
  editor-anchored thread card. It is positioned from the text range geometry,
  not the cursor, stays available while the pointer moves within the anchor or
  card, and dismisses after the pointer leaves both. It never changes the
  editor's semantic focus.
- **CMT-016:** The editor-anchored card is the sole interaction surface for
  creating, replying to, editing, resolving, reopening, and deleting normal
  comment threads. The Inspector is the navigable document-wide index.
