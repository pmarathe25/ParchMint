# UI verification

**Purpose:** Capture UI screenshots and compare PNGs. The library is
framework-neutral; the CLI uses production Iced captures. The
[UI driver](../parchmint-ui-driver/README.md) tests behavior, and
[usability review](../parchmint-ui-driver/USABILITY.md) judges complete tasks.
Pixel similarity measures rendering, not usability.

## Capture and compare

`parchmint-ui-verify` supports `list`, `capture`, `native-capture`, and `compare`.
Use the pinned toolchain and locked Cargo commands with one job:

```console
cargo run --locked -j 1 -p parchmint-ui-verification -- capture --target launcher-default --appearance light --output-stem /tmp/launcher
cargo run --locked -j 1 -p parchmint-ui-verification -- compare --reference /tmp/baseline.png --actual /tmp/launcher-tiny-skia.png --diff /tmp/diff.png --report /tmp/comparison.json
```

Create output parent directories first. `capture` renders a deterministic
1440 × 900 logical viewport at 2×, writes `<stem>-tiny-skia.png`, and refuses
existing output. `list` prints targets and appearances.

## Capture a native window

Build the desktop separately using the [release build](../../README.md#run-from-source).
Then capture its render target after three completed draws:

```console
cargo run --locked -j 1 -p parchmint-ui-verification -- native-capture --desktop target/release/parchmint --target launcher-default --appearance light --scale 1 --output /tmp/native-launcher.png
```

Application or editor-mount failures stop capture with an error. Project targets
require `--project` and navigate a real project; split-pane and recovery fixtures
are not fabricated.

The compositor may clamp window dimensions. `--require-size WIDTHxHEIGHT` rejects
mismatches after writing the actual screenshot. Supply `--logical-width` and
`--logical-height` together; `--scale` accepts 1 or 2. Optional `--reference`,
`--diff`, and `--report` must also be supplied together.

## Library and acceptance rules

[lib.rs](src/lib.rs) exposes `RgbaImage`, PNG encoding/decoding, `compare`,
`diff_image`, `write_report`, and `passes_acceptance`. Reports use schema
`parchmint.ui-verification/v1` and record dimensions, differing pixels, channel
errors, and structural metrics. Input normalizes to RGBA8; indexed images and
oversized buffers are rejected.

Exact equality passes. Same-size images also pass when all structural errors
stay within these thresholds, using a 32 × 32 area-averaged grid and 8 × 8 tiles:

| Metric | Global limit | Tiled limit |
| --- | --- | --- |
| Luminance/chroma error | 0.015 | 0.02 |
| Alpha error | 0.01 | 0.01 |

Dimension mismatches fail. Diffs mark changed pixels magenta, reference-only
pixels blue, actual-only pixels red, and matching pixels transparent.
`compare` exits 0 for acceptance, 1 for differences, and 2 for invalid input or
output. Capture failures exit 2. Comparison refuses to overwrite either input.
