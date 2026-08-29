# Comments and annotations

- **CMT-001:** Documents support range, cursor-position, and document-level comments.
- **CMT-002:** A comment is a thread with a root message and collapsible chronological replies. Every thread visibly distinguishes unresolved and resolved state without relying on color alone.
- **CMT-003:** Each thread provides an in-thread reply composer. Users can add replies, edit/delete messages, resolve, and reopen. Threads appear in one list without separate resolved/unresolved sections.
- **CMT-004:** Comment bodies are plain text with paragraph breaks.
- **CMT-005:** Comments appear in the active document's Inspector and at editor anchors. A document Inspector always includes Comments or an explicit empty state.
- **CMT-006:** Selecting a comment scrolls to and highlights its anchor in the last-focused view of that document.
- **CMT-007:** The editor context menu provides Add Comment for the current selection or cursor. Selecting text does not add a floating affordance.
- **CMT-008:** With no selection, Add Comment creates a position comment; the Comments panel can create a document-level comment.
- **CMT-009:** Comments are stored in JSON sidecars outside canonical HTML prose.
- **CMT-010:** Text anchors include stable block ID, range, quotation, and context sufficient for conservative reattachment.
- **CMT-011:** Editor changes map anchors. If recovery or transformation is ambiguous, the comment becomes orphaned and must not attach to uncertain text.
- **CMT-012:** Orphaned comments remain visible and can be reattached or converted to document-level.
- **CMT-013:** Comments are not copied when a document is duplicated.
- **CMT-014:** Comments are excluded from export and v1 global search.
- **CMT-015:** Hovering an attached range or position anchor shows a transient,
  read-only card with its quote and root comment. The card dismisses when the
  pointer moves away and never takes editor focus.
