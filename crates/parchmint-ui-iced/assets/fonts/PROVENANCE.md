# Production font assets

These static TTFs are vendored for deterministic registration on Windows,
macOS, and Linux. Wiring them into Iced is intentionally outside this asset
package.

## Source Sans 3

- Official upstream: <https://github.com/adobe-fonts/source-sans>
- Retrieved ref: `release` at `87b37a2daaed80fcb8e8ccb0085c4d72ddade12e`
- Corresponding upstream release tag: `3.052R`
  (`5d173ba058bda87bcff2bb2d53b9d2c59d440ff6`)
- License: SIL Open Font License 1.1; full text: `source-sans-3/LICENSE.md`

| Local file | Upstream path | Family / style | Intended weight | `fc-scan` weight |
| --- | --- | --- | ---: | ---: |
| `source-sans-3/SourceSans3-Regular.ttf` | `TTF/SourceSans3-Regular.ttf` | Source Sans 3 / Regular | 400 | 80 |
| `source-sans-3/SourceSans3-Medium.ttf` | `TTF/SourceSans3-Medium.ttf` | Source Sans 3 / Medium | 500 | 100 |
| `source-sans-3/SourceSans3-Semibold.ttf` | `TTF/SourceSans3-Semibold.ttf` | Source Sans 3 / Semibold | 600 | 180 |
| `source-sans-3/SourceSans3-Bold.ttf` | `TTF/SourceSans3-Bold.ttf` | Source Sans 3 / Bold | 700 | 200 |

## Source Serif 4

- Official upstream: <https://github.com/adobe-fonts/source-serif>
- Retrieved ref: `release` at `5f220b17d27ed64873f22cde0dd593685387bd19`
- Corresponding upstream release tag: `4.005R`
  (`2823e993c53fca27c5c8749f529b56a5a7c77b6b`)
- License: SIL Open Font License 1.1; full text: `source-serif-4/LICENSE.md`

| Local file | Upstream path | Family / style | Intended weight | `fc-scan` weight |
| --- | --- | --- | ---: | ---: |
| `source-serif-4/SourceSerif4-Regular.ttf` | `TTF/SourceSerif4-Regular.ttf` | Source Serif 4 / Regular | 400 | 80 |

## Verification

`SHA256SUMS` records every vendored file, including both upstream license
texts. Each font was checked as static TrueType data with `file`; its family,
style, and Fontconfig numeric weight were checked with `fc-scan`.
