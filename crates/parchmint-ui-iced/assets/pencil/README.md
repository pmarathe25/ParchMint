# Application pencil icons

These assets come from the supplied pencil icon pack and its corrected additions.
The original icon artwork is normalized to a centered optical box with six
translucent contours by `scripts/normalize-pencil-icons.py`.

Small formatting symbols use optical-size variants: B/I/U/strikethrough, lists,
alignment, line spacing, and block quote share a 24 px grid and 1.8 px strokes.
They omit tracing grain so their shapes remain distinct at 20 px. Rebuild these
variants with `python3 scripts/formatting-icons.py` after normalizing the pack.
All symbols contain paths rather than live font glyphs or embedded bitmaps.

`src/icons.rs` provides the common dimensions and muted brown light/dark tints.
The ParchMint brand artwork is separate. New tab deliberately uses the familiar
plus control instead of the supplied new-tab artwork.
