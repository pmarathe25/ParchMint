# ParchMint UI verification

Exact PNG verification for visual artifacts. The library is framework-neutral;
the command also invokes ParchMint's feature-gated `iced` capture boundary.

```text
parchmint-ui-verify capture --target launcher --appearance light --output-stem artifacts/launcher-light
parchmint-ui-verify compare --reference baseline/launcher-light-tiny-skia.png --actual artifacts/launcher-light-tiny-skia.png --diff artifacts/launcher-light-diff.png --report artifacts/launcher-light-report.json
```

`capture` prints its actual renderer-suffixed output path (for example,
`artifacts/launcher-light-tiny-skia.png`) and refuses to replace it. `compare`
writes a high-contrast PNG diff and a `parchmint.ui-verification/v1` JSON report.
It exits `0` for an exact match, `1` for a visual difference, and `2` for invalid
arguments, unreadable inputs, or output paths that could overwrite an input image.
It never creates or replaces a reference image.

Both current targets use a fixed 1440 x 900 logical viewport at 2x scale, so
captures are 2880 x 1800 pixels. Parent directories for comparison outputs must
already exist.

The checked-in Penpot references are under `references/penpot/`. Their
`reference-set.toml` records the source board for each Light and Dark fixture.
