# `iced_tiny_skia` 0.14.0

**Purpose:** Fix transform scaling and Canvas text clipping in ParchMint's patched
renderer, and avoid unnecessary CPU painting. This directory contains source
and normalized Cargo metadata from the
[`iced_tiny_skia` 0.14.0 package](https://crates.io/crates/iced_tiny_skia/0.14.0).
The original package checksum is
`fe0acf8b75a3bc914aff5f2329fdffc1b36eeaea29dda0e4bd232f1c62e9cc3d`.
Registry bookkeeping and the package lockfile are omitted.

## Transform scaling

ParchMint backports the transform composition used by the official
[`tiny_skia/src/lib.rs` on Iced `master`](https://github.com/iced-rs/iced/blob/master/tiny_skia/src/lib.rs),
as inspected on 2026-08-11. The transform backport changes `Renderer::draw`:

- Primitive-group clip bounds are scaled directly. The group transform is
  already represented in the recorded clip bounds.
- Primitive and text group transforms compose the physical scale first:
  `Transformation::scale(scale_factor) * group.transformation()`.

This keeps logical group translations subject to the viewport scale. For
example, a Canvas translated to logical x=100 with a marker at local x=20 is
drawn at physical x=240 at 2x, instead of x=140.

## Text clipping

Canvas text groups and caches are clipped to their allocated bounds.
`layer.rs` records their transformed clip bounds, matching primitive groups;
`Renderer::draw` intersects those bounds with the current layer before drawing
text. This prevents overscan text from painting across adjacent panes or the
status bar, including at 2x scale. The renderer's pixel regression covers both
live and cached text groups at 1x and 2x.

Drawing reuses the clipping mask while its bounds are unchanged. All mask writes
share one key, which resets each paint pass. This avoids clearing a window-sized
buffer for every editor glyph or line.

The glyph cache also remembers glyphs with no pixels, such as spaces, using the
same eviction policy. Blank text no longer repeats font rasterization each frame.

## Solid backgrounds

Large, pixel-aligned opaque quads fill their flat middle directly and rasterize
only the corner rows. Temporary masks are limited to those rows. Fractional
layouts, borders, shadows, gradients, and translucent fills keep the original
path. Pixel tests compare both paths at 1×, 1.25×, 1.5×, and 2×.
Pixel-aligned opaque Canvas rectangles also fill their clipped area without a mask.

## Incremental painting

Damage comparison skips unchanged Canvas items and uses actual glyph bitmap
bounds for cached text. Painting skips glyphs outside the damaged region and
omits masks for fully contained glyphs. Dirty regions include old and new ink,
stroke extents, and edge pixels. Regression tests compare incremental frames
against full repaints through edits, clipping, and fractional movement.
Immutable text groups reuse their ink bounds; unused entries are dropped each
paint pass, and loading fonts invalidates them. No rasterized-line images are kept.

Shadow damage includes both the card and its shadow. Painting clips shadow buffers
to dirty, on-screen pixels. Reused window buffers retain their own background and
scale; resize and presentation failures invalidate their history.
Antialiased vector edges use consistent blending in full and partial repaints.

## Verify the patch

From the workspace root:

```console
cargo test -p parchmint-ui-iced -p iced_tiny_skia --lib --locked -j 1
```

To check every native frame against a full repaint, including during motion:

```console
cargo build --release --locked -j 1 -p parchmint-desktop --features renderer-verification
```

Run this binary on a disposable project using the
[UI-review skill](../../.agents/skills/parchmint-ui-review/SKILL.md). Any pixel
mismatch beyond one color level of antialias rounding stops the application;
alpha must match exactly. Set `PARCHMINT_RENDER_FAILURE` to an existing temporary
directory to save failure images. This diagnostic adds full-frame CPU and memory
overhead: do not benchmark it. Rebuild without the feature for normal use.

## License and removal

The package declares the MIT license in `Cargo.toml`. The crates.io package did
not include a license file. The official Iced license text is available at
<https://github.com/iced-rs/iced/blob/master/LICENSE>.

Remove this patch and its workspace Cargo override when the selected upstream
version includes these fixes.

## SVG cache and tint

Partial repaints retain parsed SVGs and rasterized icon sizes instead of evicting
icons outside the damaged area. The cache caps parsed trees at 128, raster variants
at 512, and raster storage at 8 MiB; exceeding a cap first removes entries unused
in the current paint pass. Icon tint alpha multiplies source alpha, so muted and
disabled icon colors retain their intended transparency. Vector tests cover both.
