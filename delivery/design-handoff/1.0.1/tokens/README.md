# Design Tokens

## Exported files

- `tokens/tokens.json` — byte-for-byte copy of the archive's
  `files/2be68822-842f-8175-8008-65eef13b0227/tokens.json`
  (SHA-256 `d0754fec54be1b9d4b10d85ce31c13ebee0466f9cf71035f6ab5376c5b92744e`).

## Penpot set/theme IDs

| Set | Theme(s) | Penpot theme id | Group |
|---|---|---|---|
| PM/Core | both (Foundation) | `560a7e2f-3e5d-80c5-8008-6b976eddd033` | Foundation |
| PM/Semantic/Light | Light | `dc55a0ad-9907-801d-8008-6a1fb40bcf7a` | Appearance |
| PM/Semantic/Common | both | — | Foundation |
| PM/Semantic/Dark | Dark | `469ffc7d-964a-806d-8008-6c323f6d3396` | Appearance |
| PM/Layout/Desktop | both | — | Foundation |
| PM/Motion | both | — | Foundation |

Active theme state recorded in the archive: `Appearance/Light` +
`Foundation/Default`; active sets: PM/Core, PM/Semantic/Light,
PM/Semantic/Common, PM/Layout/Desktop, PM/Motion. `System` resolves to Light or
Dark at runtime and is not a third value set (APPR-002).

## Export method/tool version

Penpot 2.17.1-RC5 native export (`manifest.json` `generatedBy`); tokens are
stored in the archive's `tokens.json` and copied byte-for-byte — no
re-serialization was applied.

## Aliases/references

- Values use DTCG `{pm.font.family.ui}`, `{pm.font.size.12}`,
  `{pm.font.weight.regular}`, `{pm.font.tracking.normal}` style references
  (typography roles in PM/Semantic/Common) and plain `$value` leaves for
  primitives.
- No unresolved references: 0 dangling `{...}` references across all 6 sets.

## Supported token types

color, typography, spacing, sizing, borderRadius, borderWidth, shadow, number,
fontFamilies, fontSizes, fontWeights, letterSpacing, opacity.

## Counts and parity

- PM/Core 68 leaves (27 color primitives, 13 spacing, 6 borderRadius,
  2 borderWidth, 2 fontFamilies, 9 fontSizes, 4 fontWeights,
  2 letterSpacing, 3 opacity).
- PM/Semantic/Light 57 leaves (55 color + 2 shadow); PM/Semantic/Dark 57 leaves
  (55 color + 2 shadow) — semantic role parity validated.
- PM/Semantic/Common 66 typography roles (incl. 39 `pm.type.compat.inter.*`
  compat roles and `note`/`dialog-title`/`annotation` roles carrying the legacy
  `sourcesanspro` family reference).
- PM/Layout/Desktop 25 sizing leaves; PM/Motion 9 number leaves.
- Total 282 leaf records.

## Normalization needed by the deterministic import pipeline

1. Resolve `{ref}` aliases deterministically (documented reference order:
   set-local, then PM/Core primitives).
2. Normalize the legacy `sourcesanspro` font id to Source Sans 3
   (`specs/font-inventory.csv` row `pm.type.ui.note`).
3. Map the 2 semantic shadow roles per theme into platform shadow tokens.
4. Ignore `$metadata` (tokenSetOrder/activeThemes/activeSets) — runtime
   appearance selection is handled by the app, not by the token file.