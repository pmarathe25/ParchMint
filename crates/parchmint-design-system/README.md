# `parchmint-design-system`

## What it does

`parchmint-design-system` generates the colors, spacing, typography, and icons
used by the UI from the Penpot design. Running the generator twice with the same
source produces the same Rust files. This keeps the UI values in code aligned
with the design.

The crate defines names for UI tokens and icons, validates the design data, and
builds complete Light and Dark themes. Other crates store the user's appearance
choice, read the operating-system theme, store UI state, and define project
styles and export CSS.

## How it works

```text
Penpot design source
  -> export standard token JSON and SVG icon geometry
  -> validate schema, aliases, hashes, and vector metadata
  -> require matching Light and Dark semantic roles
  -> sort and normalize deterministically
  -> generate framework-neutral Rust data
```

Tokens use the Design Tokens Community Group JSON format. A token alias is a
reference to another token. The generator resolves aliases inside the selected
theme and then resolves shared tokens. It reports an error for a missing alias,
reference cycle, duplicate name, missing icon, changed checksum, or a token that
exists in only one theme. The generator normalizes the Penpot font name
`sourcesanspro` to `Source Sans 3`. SVG icons keep their `viewBox` and source
checksum. They remain vectors and receive semantic color at render time; the
generator does not create a copy for each theme.

The `System` appearance choice is resolved to Light or Dark by
`parchmint-preferences`. Here `GeneratedDesignSystem::theme_snapshot` selects
the Light or Dark values for a named appearance and records the generation
number; `parchmint-ui-api` then fans each numbered snapshot out to every open
window before it redraws.

## Interface

```rust
pub fn generate(source: DesignSource) -> Result<GeneratedDesignSystem, GenerationError>;

impl DesignSource {
    pub fn from_token_json_and_icons(
        token_json: impl Into<String>,
        icons: Vec<(String, String)>, // (name, svg); checksums are computed
    ) -> Self;
    pub fn with_token_checksum(self, checksum: impl Into<String>) -> Self;
    pub fn with_icon_checksum(self, name: &str, checksum: impl Into<String>) -> Self;
}

impl GeneratedDesignSystem {
    pub fn token(&self, name: &str) -> &SemanticToken;   // name(), token_type(), value()
    pub fn icon(&self, name: &str) -> Option<&VectorIcon>; // name(), view_box(), checksum()
    pub fn icon_catalog(&self) -> &IconCatalog;
    pub fn source_digest(&self) -> &str;
    pub fn generated_rust(&self) -> &str;
    pub fn theme_snapshot(&self, appearance: &str, generation: u64) -> ThemeSnapshot;
}

impl ThemeSnapshot {
    pub fn appearance(&self) -> &str;
    pub fn generation(&self) -> u64;
    pub fn token(&self, role: &str) -> Option<&str>;
    pub fn role_names(&self) -> Vec<&str>;
    pub fn icon_catalog(&self) -> &IconCatalog;
}

impl VectorIcon {
    pub fn is_monochrome(&self) -> bool; // always true; colored by semantic roles at render time
}
```

`GenerationError` reports `InvalidTokenSource`, `InvalidToken`,
`TokenChecksumMismatch`, `DuplicateToken`, `MissingAlias`, `AliasCycle`,
`MissingThemeRole`, `MissingSemanticRole`, `MissingIcon`, `DuplicateIcon`,
`VectorChecksumMismatch`, and `InvalidSvg`.

The crate also exposes the checked-in production surface, regenerated from the
Penpot export without opening the archive at runtime: `production_token(name)`
returns a `GeneratedToken` (name, token type, Light and Dark values),
`production_icon_svg(name)` returns a source-authored SVG string,
`PRODUCTION_ICON_NAMES` lists every catalog entry, and
`validate_production_tokens()` checks that the two required semantic roles
have both appearances.

`parchmint-ui-iced` compiles the checked-in `TOKENS` and `production_token`
roles into its private `ParchMintTheme`, so the UI needs neither the generator
nor the Penpot archive at runtime.

## Implementation

Generation is a small deterministic compiler:

```rust
pub fn generate(source: DesignSource) -> Result<GeneratedDesignSystem, GenerationError> {
    verify_checksum(&source.token_checksum, source.token_json.as_bytes())?;
    let raw_tokens = parse_tokens(&source.token_json)?;
    require_semantic_roles(&raw_tokens)?;
    let tokens = resolve_tokens(&raw_tokens)?;
    let icons = parse_icons(source.icons)?;
    let source_digest = source_digest(&source.token_json, &icons);
    let generated_rust = render_rust(&tokens, &icons, &source_digest);
    Ok(GeneratedDesignSystem {
        tokens,
        icons,
        source_digest,
        generated_rust,
    })
}
```

`verify_checksum` recomputes the SHA-256 of the token JSON against the recorded
checksum, `parse_icons` checks each SVG checksum plus its `viewBox` and rejects
`<image>` elements, and `render_rust` emits sorted `SOURCE_DIGEST`, `TOKENS`,
and `ICONS` constants. The checked-in production module records the Penpot
source and its `PENPOT_TOKEN_SOURCE_SHA256`; the generated data itself sorts
its output, so rebuilding the same source produces the same bytes. UI
components request named roles such as primary text or panel background.
Changing the application theme updates the UI. It does not change the project
files, undo history, project History, project styles, or exported documents.
