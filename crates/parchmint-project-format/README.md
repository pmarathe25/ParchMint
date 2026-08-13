# `parchmint-project-format`

## What it does

This crate reads and writes ParchMint project files. It gives each value one
standard byte representation, called its canonical form. The crate also upgrades
older project formats.

The current saved project consists of these files:

```text
project.toml
styles.css
dictionary.txt
manuscript/**/*.html
research/**/*.html
annotations/<document-id>.json
.parchmint/format-version
```

`project.toml` is the project manifest. It defines identity, hierarchy, order,
titles, Synopsis and metadata values, field definitions, style semantic
metadata, export settings, the fixed v1 `en-US` language value, deletion
tombstones, and the paths of the other project files. ParchMint reads this
manifest to learn which files belong to the project. It stores Git History,
recovery records,
caches, and workspace layout outside the canonical resource set. History uses
the Git repository at the project root, recovery uses `.parchmint/recovery/`,
the search cache uses `.parchmint/cache/`, word-count summaries are persisted in
the manifest, and workspace layout uses the platform application-data directory
keyed by project ID.

Each document's comments are stored in `annotations/<document-id>.json`. The
codec reads and writes that file together with the matching document body. An
open document's shared editor session stores the editable text and comments.

## How it works

```text
bytes -> detect format -> parse -> sanitize -> validate -> project values
                                                        |
canonical bytes <- stable encoding and hashing <--------+
```

A migration first validates the complete old project. The codec then produces
the complete new resource set in memory and hands it to the normal save path,
which writes the set as one transaction. Recording a pre-migration HistoryStore
checkpoint is not yet implemented.

## Interface

```rust
pub trait CanonicalCodec: Send + Sync {
    fn detect(&self, control: &[u8]) -> Result<FormatVersion, FormatError>;
    fn decode_project(&self, input: CanonicalInputSet)
        -> Result<ProjectModel, FormatError>;
    fn decode_document(&self, bytes: &[u8])
        -> Result<CanonicalDocument, FormatError>;
    fn decode_annotations(&self, bytes: &[u8])
        -> Result<CanonicalAnnotations, FormatError>;
    fn encode(&self, value: &CanonicalResource)
        -> Result<CanonicalBytes, FormatError>;
    fn migrate(&self, source: SourceFormatSnapshot, target: FormatVersion)
        -> Result<CanonicalResourceSet, MigrationError>;
}

pub struct CanonicalBytes {
    pub resource: ResourceId,
    pub path: CanonicalRelativePath,
    pub bytes: Vec<u8>,
    pub hash: ContentHash,
}
```

[`parchmint-contracts`](../parchmint-contracts/README.md) defines the JSON annotation
shape. This crate owns the HTML, TOML, CSS, and text codecs, checks the project
rules, and writes their standard byte forms.

`ProjectFormatCodec` is the concrete v1 codec. In addition to the trait, it
assembles and decodes whole domain projects and persistence frontiers
(`encode_domain_project*`, `decode_manifest`, `decode_styles`,
`decode_dictionary`, `decode_domain_project*`, `decode_persistence_frontier`).

## Implementation

- All text uses UTF-8 and LF.
- Record, attribute, whitespace, escaping, ID, and dictionary order is stable.
- Equivalent values have one byte representation. Re-encoding an already
  canonical document is byte-identical, so a save never rewrites an unchanged
  document with different formatting.
- Canonical HTML allows only the supported semantic blocks and marks. Scripts,
  event handlers, remote embeds, arbitrary inline styles, and unsafe links are
  rejected before encoding.
- Canonical paths are relative and reject traversal, case collisions, and
  Unicode-normalization collisions.
- Unknown newer formats and invalid inputs fail without changing project files.
- A migration preserves stable IDs when their meaning is unchanged. SQLite,
  recovery, workspace, and editor-native state are outside migration input.
