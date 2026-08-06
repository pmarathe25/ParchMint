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

The `System` appearance setting resolves to either the Light or Dark theme. A
`ThemeSnapshot` contains the selected theme and a generation number. Every open
window receives the new snapshot before it redraws.

## Public API

```rust
pub enum ResolvedAppearance {
    Light,
    Dark,
}

pub struct ThemeSnapshot {
    pub appearance: ResolvedAppearance,
    pub generation: u64,
    pub tokens: &'static SemanticTokenSet,
    pub icons: &'static IconCatalog,
}

pub fn theme_snapshot(
    appearance: ResolvedAppearance,
    generation: u64,
) -> ThemeSnapshot;

pub fn icon(id: SemanticIcon) -> &'static VectorIcon;
```

`parchmint-ui-iced` converts `ThemeSnapshot` into its private `iced` style
types.

## Implementation

Generation is a small deterministic compiler:

```rust
fn generate(source: DesignSource) -> Result<GeneratedDesignSystem> {
    source.verify_hashes()?;
    let tokens = resolve_aliases(source.tokens)?;
    require_light_dark_parity(&tokens)?;
    let icons = verify_indexed_vectors(source.icons)?;

    Ok(GeneratedDesignSystem {
        tokens: tokens.into_sorted_metadata(),
        icons: icons.into_sorted_catalog(),
        source_digest: source.digest(),
        generator_version: GENERATOR_VERSION,
    })
}
```

Generated files record the source name, source hash, and generator version. The
generator sorts its output, so rebuilding the same source produces the same
bytes. UI components request named roles such as primary text or panel
background. Changing the application theme updates the UI. It does not change
the project files, undo history, project History, project styles, or exported
documents.
