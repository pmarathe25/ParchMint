# `parchmint-project-format`

**Purpose:** Encode project values deterministically and decode validated project
files. A value's standard byte representation is its *canonical form*.

## Project resources

```text
project.toml
styles.css
dictionary.txt
manuscript/**/*.html
research/**/*.html
annotations/<document-id>.json
.parchmint/format-version
```

`project.toml` lists project identity, hierarchy and order, titles, Synopsis,
metadata values and field definitions, style semantics, export settings,
word-count summaries, deletion tombstones, resource paths, and the fixed `en-US`
language. The manifest determines which files belong to the project.

Each document body is read and written with its annotation sidecar. Open editor
sessions own editable text and comments. History, recovery, caches, and workspace
layout are separate from canonical resources; see
[data locations](../../docs/architecture/architecture.md#data-locations-and-ownership).

## Interface

`CanonicalCodec` detects formats, decodes resources, and encodes canonical bytes.
`ProjectFormatCodec` implements the current v1 format and assembles domain
projects and persistence revision lists. Its concrete methods decode manifests,
styles, dictionaries, and saved revisions, and encode complete domain projects.
See [lib.rs](src/lib.rs).

[contracts](../parchmint-contracts/README.md) defines annotation JSON records.
This crate owns HTML, TOML, CSS, and text parsing, sanitization, validation,
encoding, and hashing.

## Encoding rules

- Text uses UTF-8 and LF. Record, attribute, whitespace, escaping, ID, and
  dictionary ordering is stable.
- Equivalent values encode identically. Re-encoding canonical content leaves its
  bytes unchanged.
- HTML accepts supported semantic blocks and marks. Scripts, event handlers,
  remote embeds, arbitrary inline styles, and unsafe links are rejected.
- Resource paths are relative; traversal, case collisions, and
  Unicode-normalization collisions are rejected.
- Invalid input and unknown newer formats fail without modifying project files.

Document decoding also derives rendered word counts for persistence and UI
summaries. Markup, attributes, and empty paragraphs do not count as prose;
adjacent blocks retain word boundaries.
