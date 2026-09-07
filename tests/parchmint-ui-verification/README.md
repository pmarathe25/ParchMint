# `parchmint-ui-verification`

This crate captures UI screenshots and compares PNGs. Its library is
framework-neutral; the command uses the production Iced capture boundary.
Application requirements live in tests. A screenshot comparison checks rendering;
the [UI driver](../parchmint-ui-driver/README.md) checks working application flows.

## Commands

`parchmint-ui-verify` supports `list`, `capture`, `native-capture`, and `compare`.
Build or run it with the pinned toolchain and `--locked -j 1`.

```console
cargo run --locked -j 1 -p parchmint-ui-verification -- capture --target launcher-default --appearance light --output-stem /tmp/launcher
cargo run --locked -j 1 -p parchmint-ui-verification -- compare --reference /tmp/baseline.png --actual /tmp/launcher-tiny-skia.png --diff /tmp/diff.png --report /tmp/comparison.json
```

`capture` renders a deterministic 1440 × 900 logical viewport at 2× scale.
It writes `<stem>-tiny-skia.png` and refuses an existing output. `list` prints
available targets and appearances. Output parent directories must exist.

`native-capture` launches a separately built desktop executable and captures its
actual render target after three completed draws of the loaded application.
Application failures stop capture with an error instead of accepting an error
screen or waiting indefinitely for a failed editor mount:

```console
parchmint-ui-verify native-capture --desktop target/release/parchmint --target launcher-default --appearance light --scale 1 --output /tmp/native-launcher.png
```

Project targets require `--project`. They select a destination in a real project;
fixture-specific split-pane and recovery states are not fabricated. The compositor
may clamp the window size. `--require-size WIDTHxHEIGHT` rejects a mismatch after
writing the true screenshot. `--logical-width` and `--logical-height` are paired;
`--scale` accepts 1 or 2. Optional `--reference`, `--diff`, and `--report` flags
must be supplied together.

## Interface and implementation

[lib.rs](src/lib.rs) provides `RgbaImage`, PNG decoding/encoding, `compare`,
`diff_image`, `write_report`, and `passes_acceptance`. Reports use schema
`parchmint.ui-verification/v1` and record dimensions, differing pixels, channel
errors, and structural metrics. Inputs normalize to RGBA8; indexed input and
oversized buffers are rejected.

Exact equality passes. Same-size differences can also pass the structural
policy: global luminance/chroma error ≤ 0.015 and alpha error ≤ 0.01; tiled
luminance/chroma error ≤ 0.02 and alpha error ≤ 0.01. Samples use a 32 × 32
area-averaged grid and 8 × 8 tiles. Dimension mismatches fail.

Diffs mark changed pixels magenta, reference-only pixels blue, and actual-only
pixels red. Matching pixels are transparent. `compare` exits 0 for acceptance,
1 for visual differences, and 2 for invalid inputs or outputs. Capture errors
exit 2. Comparison refuses outputs that would overwrite either input.
