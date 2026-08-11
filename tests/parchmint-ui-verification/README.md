# ParchMint UI verification

Exact PNG verification for visual artifacts. The library is framework-neutral.
`capture` remains a deterministic `iced_test`/tiny-skia composition check; it
does not open the production desktop and is not native visual evidence.

`native-capture` launches the production `parchmint` executable and uses its
Iced 0.14 `window::screenshot` render-target flow. It requests a 1440 x 900
logical window at 2x scale by default. The desktop waits
for three non-blocking settled-frame ticks, encodes RGBA PNG in a background
task, refuses an existing output, reports the actual render-target dimensions,
and exits by default. A compositor may clamp the requested window; the default
still writes the true screenshot so comparison emits a dimension-mismatch
report and diff. Use `--require-size 2880x1800` to make that mismatch fail the
desktop command after it writes the image. `--scale 1|2` and paired
`--logical-width`/`--logical-height` options configure the request.

```text
parchmint-ui-verify native-capture \
  --desktop target/debug/parchmint \
  --target launcher-default --appearance light \
  --output /tmp/parchmint/launcher-light.png \
  --reference references/penpot/light/launcher-light.png \
  --diff /tmp/parchmint/launcher-light-diff.png \
  --report /tmp/parchmint/launcher-light-report.json

parchmint-ui-verify native-capture \
  --desktop target/debug/parchmint \
  --target cards-default --appearance dark \
  --project /absolute/path/to/project.parchmint \
  --output /tmp/parchmint/cards-dark.png \
  --reference references/penpot/dark/cards-dark.png \
  --diff /tmp/parchmint/cards-dark-diff.png \
  --report /tmp/parchmint/cards-dark-report.json
```

Native project targets select the real project's matching `RibbonDestination`:
`editor`, `cards`, `global-search`, `history`, `settings`, `export`, and
`recently-deleted`. `editor-single-default`, `editor-dual-default`, and
`error-recovery-default` all select the production Editor destination; they do
not fabricate those fixture-only states. Diff/report paths must already have
parent directories and are never allowed to overwrite either input image.

```text
parchmint-ui-verify capture --target launcher --appearance light --output-stem artifacts/launcher-light
parchmint-ui-verify compare --reference baseline/launcher-light-tiny-skia.png --actual artifacts/launcher-light-tiny-skia.png --diff artifacts/launcher-light-diff.png --report artifacts/launcher-light-report.json
```

`capture` prints its actual renderer-suffixed output path (for example,
`artifacts/launcher-light-tiny-skia.png`) and refuses to replace it. `compare`
writes a high-contrast PNG diff and a `parchmint.ui-verification/v1` JSON report.
Both `compare` and `verify-catalog` use the same policy: exact equality passes;
otherwise the report must stay within the documented global and tiled structural
thresholds. They exit `0` for acceptance, `1` for a visual difference, and `2`
for invalid arguments, unreadable inputs, or output paths that could overwrite
an input image. They never create or replace a reference image.

Run the complete headless catalog gate with a fresh artifact directory:

```text
parchmint-ui-verify verify-catalog \
  --references references/penpot \
  --output /tmp/parchmint-ui-catalog
```

It renders all 10 targets in Light and Dark at 2880 x 1800, writes each actual
PNG, magenta diff, detailed JSON report, and `catalog-index.json`. Existing
catalog artifacts are refused and references are read-only. The command exits
`1` if an exact comparison fails and the strict 32 x 32 structural metric also
exceeds its documented global 0.015 luminance/chroma MAE, 0.01 alpha MAE, or
tiled 0.02 luminance/chroma MAE and 0.01 alpha MAE thresholds. The current
Penpot catalog is expected to remain red until UI remediation; its JSON reports
identify the exact and structural metrics rather than accepting current output.

Both current targets use a fixed 1440 x 900 logical viewport at 2x scale, so
captures are 2880 x 1800 pixels. Parent directories for comparison outputs must
already exist.

The checked-in Penpot references are under `references/penpot/`. Their
`reference-set.toml` records the source board for each Light and Dark fixture.
