# Canonical project data

- **DATA-001:** Current authored data uses restricted deterministic HTML5, TOML, CSS, JSON, and UTF-8 text sidecars.
- **DATA-002:** Groups map to directories and documents to HTML files, while the manifest is authoritative for identity, ordering, titles, metadata, and semantics.
- **DATA-003:** Internal filenames are implementation details and never normal UI labels.
- **DATA-004:** Renaming a displayed title does not rename the backing file.
- **DATA-005:** Serialization uses UTF-8, LF, deterministic attribute ordering, stable whitespace/escaping, stable block IDs, and no rewriting of unchanged documents.
- **DATA-006:** Deleting caches and indexes does not break current project functionality.
- **DATA-007:** Deleting history removes old versions but does not damage current authored content.
- **DATA-008:** Canonical paths are relative, normalized, traversal-safe, case-conflict checked, and portable across Windows/macOS/Linux.
- **DATA-009:** GUI/editor-engine-native state is transient and never canonical project format.
- **DATA-010:** SQLite is derived state only and never the sole project store.
- **DATA-011:** The project dictionary is stored in a deterministic, inspectable canonical text representation. The global dictionary is stored in application preferences outside the project.
