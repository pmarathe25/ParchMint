# `iced_tiny_skia` 0.14.1

**Purpose:** Fix Canvas text clipping in ParchMint's patched renderer and avoid
unnecessary CPU painting. This directory contains source
and normalized Cargo metadata from the
[`iced_tiny_skia` 0.14.1 package](https://crates.io/crates/iced_tiny_skia/0.14.1).
The original package checksum is
`c267596d742714b1853cc10c3983a367762816fc4836bd3b79f76ce76787d6f8`.
Registry bookkeeping and the package lockfile are omitted.

## Transform scaling

Iced 0.14.1 includes the transform composition that ParchMint previously
backported. The renderer now follows the upstream behavior:

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
Border-only quads skip their fully transparent interior fill while keeping the
border and shadow. This avoids rasterizing large empty card interiors.

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

Grouped dirty regions can still overlap. When their summed area reaches the
viewport area, the compositor instead paints the whole viewport once. This
avoids repeatedly traversing and painting the same layers during Overview
scrolling; smaller updates continue to use partial repainting.
Repeated presentations of an unchanged scene reuse the last completed pixels
when softbuffer supplies an older back buffer. This keeps normal presentation
and frame scheduling while avoiding another raster pass. The cache holds one
window-sized pixel buffer and is cleared on resize or presentation failure.

On the nested `Manual-Test-Project-Ready.parchmint` Overview at a 1280×720
viewport, one scroll produced 147 raw dirty rectangles. Grouping left 16
regions totaling 1.45 million pixels, compared with 0.92 million viewport
pixels. In a 20-step alternating scroll run, the median software raster pass
fell from about 30 ms to 10 ms after the full-repaint choice. An unchanged
second presentation then used a roughly 1 ms pixel copy instead of another
roughly 10 ms raster pass. These timings are machine-specific; they explain the
choice rather than define a performance contract.

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
[UI-review skill](../../.agents/skills/parchmint-ui-review/SKILL.md) with
`ICED_BACKEND=tiny-skia`. Any pixel
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
