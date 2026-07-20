# Editor platform verification charter

Run this charter on Windows, macOS, and Linux whenever the Qt minor line or
editor host changes. Record OS, input source, keyboard layout, display scale,
Qt version, and results in the stage handoff.

1. Type dead-key sequences for accented Latin text and undo/redo them.
2. Compose at least one non-Latin phrase through the operating-system IME,
   including candidate selection and pre-edit cancellation.
3. Navigate emoji ZWJ sequences, combining marks, Indic conjuncts, Arabic, and
   Hebrew by grapheme and word; extend selections in both directions.
4. Paste fixture plain text and confirm Markdown punctuation remains literal.
5. Paste rich HTML and confirm supported emphasis/link structure survives while
   active content does not.
6. Open two editors, create different selections and undo stacks, then verify
   edits and undo in one do not change the other.
7. Repeat at 100%, 150%, and 200% scale and with a screen reader enabled.
8. Copy/paste a page break, asset image, and protected opaque block through the
   native platform rich clipboard; confirm identity/source properties survive
   or the paste visibly degrades to supported plain content.
9. Backspace immediately before and after a page break/opaque object, then
   delete a selection containing it. Confirm each object is indivisible and a
   selection deletion is the only bulk-delete path.
10. While composing through the IME, allow the autosave debounce and change
    focus. Confirm pre-edit text is not canonicalized as committed text and the
    focus-loss journal contains the final committed revision.
11. Switch to raw source, introduce an unclosed front matter/fence, and confirm
    the buffer remains available with diagnostics. Fix or explicitly discard it
    and verify the WYSIWYG undo boundary.
12. Modify a clean then dirty open Markdown file in an external editor. Confirm
    clean content reloads and dirty content presents compare/reload/overwrite/
    save-copy with no preselected destructive action.

Automated Qt Test covers semantic formats, independent documents, distinct
plain/rich paste paths, grapheme boundaries, and synthetic Japanese IME/dead-key
pre-edit/commit events. Physical input sources and screen readers remain manual
evidence because synthetic events do not exercise platform input-method plugins.

Stage 03 Linux automated evidence (2026-07-19, Qt 6.8.3 offscreen) passes mixed
formatting, semantic load/snapshot, visible protected-object registration,
revisioned dirty ranges, focus-loss flush requests, independent undo, Unicode
graphemes, rich/plain paste, and synthetic Japanese/dead-key composition.
Physical Linux input and every Windows/macOS item remain unrecorded.
