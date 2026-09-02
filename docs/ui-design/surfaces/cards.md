# Cards

- Cards show the complete hierarchy vertically with indentation. Title,
  Synopsis, and metadata are read-only projections; Inspector is their single
  editing surface. Each fixed-height row uses concise single-line previews for
  long or multiline values; Inspector exposes their complete text. Render each
  row as a compact bordered tile. Render every applicable metadata field as a
  labelled `Field: value` chip whose theme-aware tint is deterministically
  derived from its normalized field label; colour only aids scanning and never
  replaces the visible label or value. Do not add an implicit `Status: Draft`,
  and show a clear insertion marker while dragging. Single-clicking a group
  card selects it and expands or collapses its children; double-clicking a
  document card switches to Editor and opens a permanent tab.
