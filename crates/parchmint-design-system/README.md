# `parchmint-design-system`

**Purpose:** Supply framework-neutral design tokens and symbolic SVG icons.
The UI maps them to Iced widgets; preferences select the appearance.

## Interface and implementation

`production_token(name)` returns a `DesignToken` with Light and Dark values.
`production_icon_svg(name)` returns an SVG using `currentColor`. `TOKENS`,
`REQUIRED_SEMANTIC_ROLES`, and `PRODUCTION_ICON_NAMES` expose the catalogs.

Edit [tokens.rs](src/tokens.rs) for colors, spacing, and typography, and
[lib.rs](src/lib.rs) for icons. This crate has no dependencies or runtime parser.
Tests check required roles in both appearances and the icon catalog. Project
styles and export CSS belong to their own crates.
