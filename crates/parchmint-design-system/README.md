# `parchmint-design-system`

This crate provides framework-neutral UI tokens and symbolic SVG icons.
The UI maps these values to Iced colors and widgets; application preferences
choose which appearance to display.

## Interface

`production_token(name)` returns a `DesignToken` with Light and Dark values.
`production_icon_svg(name)` returns an SVG using `currentColor`.
`TOKENS`, `REQUIRED_SEMANTIC_ROLES`, and `PRODUCTION_ICON_NAMES` expose the catalogs.

## Implementation

Edit [tokens.rs](src/tokens.rs) for colors, spacing, and typography, and
[lib.rs](src/lib.rs) for icons. The crate has no dependencies or runtime parser.
Tests check required roles in both appearances and the icon catalog.
Project styles and export CSS belong to their own crates.
