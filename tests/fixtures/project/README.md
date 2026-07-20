# Project-format focused fixtures

The storage tests create full temporary projects so IDs and paths are generated
through the domain command layer. These small files supplement those tests with
reviewable field/diagnostic cases:

- `styles-all-fields.toml` covers paragraph/character styles, inheritance,
  `next_style`, bounded properties, and a namespaced compile preset.
- `assets-valid.toml` covers the independently versioned attachment catalog.
- `assets-duplicate-id.toml`, `outline-unsafe.toml`, and
  `parchmint-newer-version.toml` are invalid/forward-compatibility inputs.

They are source fixtures, not alternate schemas. Their spelling follows
[`project-format-1.md`](../../../docs/format/project-format-1.md).
