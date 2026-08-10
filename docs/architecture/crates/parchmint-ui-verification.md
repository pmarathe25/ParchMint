# `parchmint-ui-verification`

## What it does

`parchmint-ui-verification` checks PNGs produced by the production UI capture
boundary. Its comparison library does not use `iced` or another UI framework.
It decodes and normalizes input PNGs to RGBA8, compares every channel exactly,
writes a high-contrast diff PNG, and writes a versioned JSON report.

The crate also provides the `parchmint-ui-verify` command. It exits `0` for an
exact match, `1` for a visual difference, and `2` for invalid arguments,
unreadable inputs, or invalid output paths. It never creates or replaces a
reference image, and refuses output paths that would overwrite either input.

## How it works

```text
approved reference PNG + newly captured PNG
              |
              v
      decode and normalize to RGBA8
              |
              v
 exact dimensions and channel metrics
              |
       +------+------+
       v             v
   diff PNG      JSON report
```

Reference images are reviewed design artifacts. The comparison command does not
generate, update, or approve references. Canonical Penpot exports are stored
under `references/penpot/` with their board IDs and capture metadata. A dimension
mismatch is a failure; per-pixel metrics are reported when dimensions match.
The report schema is `parchmint.ui-verification/v1`.

## Capture boundary

`parchmint-ui-iced` owns capture because it owns the production-native views.
Its `visual-verification` feature is non-default and uses the pinned headless
`iced` tiny-skia renderer. Capture is fixed at 2x physical scale for the
current production-native targets. Launcher and Project both use a 1440 x 900
logical verification viewport and produce 2880 x 1800 physical pixels. This
viewport is independent of each native window's default launch size. The
capture boundary rejects an existing output path. It exposes no fixture-only
surface.

An approved capture harness calls `parchmint_ui_iced::capture_visual` with a
target, appearance, and output stem. It writes a new
`<stem>-tiny-skia.png`; comparison remains the responsibility of this crate.
The comparator accepts a checked-in Penpot reference or a separately approved
application regression baseline. It never changes either input.

## Commands

Capture a production view, then compare the new PNG with an existing approved
reference. The output directories must already exist.

```text
cargo run --locked -p parchmint-ui-verification -- capture --target launcher --appearance light --output-stem captures/launcher-light
cargo run --locked -p parchmint-ui-verification -- compare --reference tests/parchmint-ui-verification/references/penpot/light/launcher-light.png --actual captures/launcher-light-tiny-skia.png --diff reports/launcher-light-diff.png --report reports/launcher-light.json
```

The first command writes the production-native headless capture and prints its
renderer-suffixed path. The second command performs the exact comparison and
writes only the requested diff and report.

## Public API

```rust
pub fn decode_png(path: impl AsRef<Path>) -> Result<RgbaImage, VerificationError>;
pub fn compare(reference: &RgbaImage, actual: &RgbaImage) -> ComparisonReport;
pub fn diff_image(reference: &RgbaImage, actual: &RgbaImage)
    -> Result<RgbaImage, VerificationError>;
pub fn write_report(path: impl AsRef<Path>, report: &ComparisonReport)
    -> Result<(), VerificationError>;
```

`ComparisonReport` records dimensions, whether dimensions differ, differing
pixel count, maximum channel delta, and mean absolute channel delta. Matching
requires identical dimensions and zero channel delta.
