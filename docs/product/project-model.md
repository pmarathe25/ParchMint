# Project model

## Project

A project's authored content lives in canonical files. ParchMint also keeps
project history and crash-recovery data, and it may build disposable data such
as a search index. Document word counts are canonical `project.toml` manifest
data. Each project has:

- Stable project ID.
- Display title.
- Optional author.
- Fixed spellcheck language value (`en-US`) for forward compatibility.
- Fixed Manuscript and Research roots.
- Ordered hierarchy.
- Project styles.
- Metadata-field definitions and values.
- Comments and annotations.
- Project dictionary.
- Export settings.
- History, recovery, and workspace state.

Appearance and the global dictionary are application preferences, not authored project state.

## Fixed roots

- **Manuscript:** Contains content included in normal manuscript export.
- **Research:** Contains app-created supporting notes excluded from normal export.

The visible root labels are fixed in v1 and cannot be renamed, deleted, copied, reordered, or moved.

## Node types

| Type | Content and behavior |
| --- | --- |
| Group | May contain groups and documents. Has a title, Synopsis, metadata, ordering, and export settings, but no editable prose body. |
| Document | Has a title, semantic rich-text body, Synopsis, metadata, comments, ordering, and export settings. Cannot contain children. |
