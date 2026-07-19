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

Automated Qt Test covers semantic formats, independent documents, distinct
plain/rich paste paths, grapheme boundaries, and synthetic Japanese IME/dead-key
pre-edit/commit events. Physical input sources and screen readers remain manual
evidence because synthetic events do not exercise platform input-method plugins.
