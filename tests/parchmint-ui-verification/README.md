# `parchmint-ui-verification`

## What it does

`parchmint-ui-verification` checks PNGs produced by the production UI capture
boundary. Its comparison library is framework-neutral: it does not use `iced`
or another UI framework. It decodes and normalizes input PNGs to RGBA8,
compares every channel exactly, writes a high-contrast diff PNG, and writes a
versioned JSON report.

The crate also provides the `parchmint-ui-verify` command with `list`,
`capture`, `compare`, `native-capture`, and `verify-catalog` subcommands.
`compare` and `verify-catalog` exit `0` for acceptance (an exact match, or a
same-size difference within the documented structural thresholds), `1` for a
visual difference that exceeds those thresholds, and `2` for invalid
arguments, unreadable inputs, or invalid output paths. `capture` and
`native-capture` exit `2` for any capture error, including an output path that
already exists. No command ever creates or replaces a reference image, and
comparison outputs are refused if they would overwrite either input.

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
under `references/penpot/` with their board IDs and capture metadata. A
dimension mismatch is a failure; per-pixel metrics are reported when dimensions
match. The report schema is `parchmint.ui-verification/v1`.

`capture` remains a deterministic `iced_test`/tiny-skia composition check; it
does not open the production desktop and is not native visual evidence.
`compare` and `verify-catalog` share one acceptance policy: exact equality
passes; otherwise the same-size report must stay within the global and tiled
structural thresholds (0.015 luminance/chroma MAE and 0.01 alpha MAE globally;
0.02 luminance/chroma and 0.01 alpha MAE over the strict tiled measure).
`verify-catalog` renders all 10 targets in Light and Dark at 2880 x 1800,
writes each actual PNG, magenta diff, detailed JSON report, and a
`parchmint.ui-verification-catalog/v1` `catalog-index.json`. Existing catalog
artifacts are refused and references are read-only. The command exits `1` if
an exact comparison fails and the structural metric also exceeds its
threshold. The current Penpot catalog is expected to remain red until UI
remediation; its JSON reports identify the exact and structural metrics rather
than accepting current output.

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
target, appearance, and output stem. It writes a new `<stem>-tiny-skia.png`;
comparison remains the responsibility of this crate. The comparator accepts a
checked-in Penpot reference or a separately approved application regression
baseline. It never changes either input.

`native-capture` launches the production `parchmint` executable and uses its
Iced 0.14 `window::screenshot` render-target flow. It requests a 1440 x 900
logical window at 2x scale by default. The desktop waits for three
non-blocking settled-frame ticks, encodes the RGBA PNG in a background task,
refuses an existing output, reports the actual render-target dimensions, and
exits by default. A compositor may clamp the requested window; the default
still writes the true screenshot so comparison emits a dimension-mismatch
report and diff. Use `--require-size 2880x1800` to make that mismatch fail the
desktop command after it writes the image. `--scale 1|2` and the paired
`--logical-width`/`--logical-height` options configure the request.

## Commands

Capture a headless verification view, then compare the new PNG with an existing
approved reference. The output directories must already exist.

```text
cargo run --locked -p parchmint-ui-verification -- capture --target launcher-default --appearance light --output-stem captures/launcher-light
cargo run --locked -p parchmint-ui-verification -- compare --reference tests/parchmint-ui-verification/references/penpot/light/launcher-light.png --actual captures/launcher-light-tiny-skia.png --diff reports/launcher-light-diff.png --report reports/launcher-light.json
```

The first command writes the headless verification capture and prints its
renderer-suffixed path (for example, `captures/launcher-light-tiny-skia.png`),
and it refuses to replace an existing renderer output. The second command
performs the exact comparison and writes only the requested diff and report;
`--diff` and `--report` must use different paths that neither input may
overwrite.

`native-capture` runs the production desktop. Project targets use a real
project:

```text
parchmint-ui-verify native-capture \
  --desktop target/debug/parchmint \
  --target launcher-default --appearance light \
  --output /tmp/parchmint/launcher-light.png \
  --reference tests/parchmint-ui-verification/references/penpot/light/launcher-light.png \
  --diff /tmp/parchmint/launcher-light-diff.png \
  --report /tmp/parchmint/launcher-light-report.json

parchmint-ui-verify native-capture \
  --desktop target/debug/parchmint \
  --target cards-default --appearance dark \
  --project /absolute/path/to/project.parchmint \
  --output /tmp/parchmint/cards-dark.png \
  --reference tests/parchmint-ui-verification/references/penpot/dark/cards-dark.png \
  --diff /tmp/parchmint/cards-dark-diff.png \
  --report /tmp/parchmint/cards-dark-report.json
```

`--output` must be an absolute PNG path that does not yet exist. Native project
targets select the real project's matching `RibbonDestination`: `editor`,
`cards`, `global-search`, `history`, `settings`, `export`, and
`recently-deleted`, and they require `--project`. `editor-single-default`,
`editor-dual-default`, and `error-recovery-default` all select the production
Editor destination; they do not fabricate those fixture-only states.
`--reference`, `--diff`, and `--report` must be supplied together; when given,
the captured PNG is compared with the same policy and exit codes as `compare`.
Diff/report paths must already have parent directories and are never allowed
to overwrite either input image.

Run the complete headless catalog gate with a fresh artifact directory:

```text
parchmint-ui-verify verify-catalog \
  --references references/penpot \
  --output /tmp/parchmint-ui-catalog
```

Existing catalog artifacts are refused. `list` prints every fixture ID,
appearance, and reference ID (20 combinations). The checked-in Penpot
references are under `references/penpot/`; their `reference-set.toml` records
the source board and export geometry for each Light and Dark fixture, and
`SHA256SUMS` pins the file contents. `verify-catalog` validates both before
rendering.

## Interface

```rust
pub const REPORT_SCHEMA: &str = "parchmint.ui-verification/v1";
pub const CATALOG_SCHEMA: &str = "parchmint.ui-verification-catalog/v1";

pub struct RgbaImage { /* width, height, tightly packed RGBA8 pixels */ }

impl RgbaImage {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>)
        -> Result<Self, VerificationError>;
    pub const fn width(&self) -> u32;
    pub const fn height(&self) -> u32;
    pub fn pixels(&self) -> &[u8];
}

pub enum VerificationError {
    Io(io::Error),
    Decode(png::DecodingError),
    Encode(png::EncodingError),
    Json(serde_json::Error),
    InvalidImageBuffer { expected: usize, actual: usize },
    ImageTooLarge,
    UnsupportedPngColorType(png::ColorType),
    OutputExists(PathBuf),
}

pub fn decode_png(path: impl AsRef<Path>) -> Result<RgbaImage, VerificationError>;
pub fn encode_png(path: impl AsRef<Path>, image: &RgbaImage) -> Result<(), VerificationError>;
pub fn compare(reference: &RgbaImage, actual: &RgbaImage) -> ComparisonReport;
pub fn diff_image(reference: &RgbaImage, actual: &RgbaImage)
    -> Result<RgbaImage, VerificationError>;
pub fn write_report(path: impl AsRef<Path>, report: &ComparisonReport)
    -> Result<(), VerificationError>;
pub fn passes_acceptance(report: &ComparisonReport) -> bool;

pub fn write_catalog_case(
    output_directory: impl AsRef<Path>,
    id: &str,
    appearance: &str,
    reference_path: impl AsRef<Path>,
    actual: &RgbaImage,
) -> Result<CatalogCaseReport, VerificationError>;
pub fn write_catalog_index(
    output_directory: impl AsRef<Path>,
    cases: &[CatalogCaseReport],
) -> Result<PathBuf, VerificationError>;
```

`ComparisonReport` records the schema, `matches`, the reference and actual
dimensions, whether dimensions differ, differing pixel count, maximum channel
delta, mean absolute channel delta, and an optional `StructuralMetrics`
section. Matching requires identical dimensions and zero channel delta; a
dimension mismatch omits the pixel metrics. `StructuralMetrics` is a
renderer-tolerant 32 x 32 sample-grid comparison with global and tiled
luminance/chroma/alpha MAE values, and `passes_acceptance` applies the
`MAX_STRUCTURAL_*` thresholds (0.015 global luminance/chroma, 0.01 alpha,
0.02 tiled luminance/chroma, 0.01 tiled alpha).

## Implementation

The library in `src/lib.rs` is framework-neutral and uses `png` plus
`serde`/`serde_json`; the command binary in `src/main.rs` is the only part that
depends on `parchmint-ui-iced`'s `visual-verification` feature.
`decode_png` expands grayscale, RGB, and 16-bit inputs, so every image reaches
`compare` as RGBA8; indexed color input is rejected and oversized images fail
before allocation. `diff_image` paints matching pixels transparent, differing
pixels magenta, reference-only pixels blue, and actual-only pixels red.

The structural sample is a fixed 32 x 32 grid of area-averaged samples with an
8 x 8 tile grid; luminance uses Rec. 709 weights and chroma uses the
(R - luma, B - luma) pair. Exact equality is authoritative; the structural
metric is a conservative fallback for renderer-tolerant same-size output.
`verify-catalog` validates `reference-set.toml` (schema, 1440 x 900 at 2x,
2880 x 1800, exactly one screen per fixture, matching light/dark paths) and
`SHA256SUMS` before rendering, and it runs each case in a fresh subprocess via
the internal `catalog-case` subcommand so memory stays bounded.
