# Penpot UI references

These PNGs are stable raster references for the canonical ParchMint screen
fixtures. The editable design authority remains
[`docs/ui-design/parchmint-ui.penpot`](../../../../docs/ui-design/parchmint-ui.penpot).

The files were exported on 2026-08-10 from the connected `ParchMint` Penpot
file. Its file, page, and board IDs matched the checked-in screen catalog. Each
1440 x 900 board was exported as a native PNG at scale 2, producing a
2880 x 1800 image.

Dark references were exported while `Appearance/Dark` and
`PM/Semantic/Dark` were active. Light was then activated, allowed to propagate,
and verified with a Launcher export before the Light batch was captured. The
active theme was checked again after each batch.

Use these files to diagnose differences between Penpot and the application.
Different renderers can rasterize fonts and edges differently, so a nonzero
pixel diff needs visual review. Once an application rendering is approved, use
that application PNG as the exact regression baseline for later code changes.

`reference-set.toml` maps every filename to its Penpot board and fixture.
`SHA256SUMS` detects accidental changes to the exported files.
