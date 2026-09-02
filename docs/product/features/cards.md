# Cards

- **CARD-001:** Cards is an alternate projection of the same hierarchy, titles, Synopsis, metadata, ordering, and selection model used by Editor mode.
- **CARD-002:** v1 uses one vertically ordered, virtualized hierarchy. A multi-column corkboard is not included.
- **CARD-003:** Cards preserve hierarchy, allow groups to expand/collapse, and expose the current drag destination.
- **CARD-004:** Dropping onto a group moves the node into that group.
- **CARD-005:** Cards supports Manuscript and Research, defaulting to Manuscript.
- **CARD-006:** Group and document Cards display title, Synopsis, and applicable configured metadata in compact, fixed-height bordered tiles. Each configured field is a labelled chip (`Field: value`); its theme-aware tint is deterministically derived from the normalized field label, is consistent across projects, and never replaces the visible label or value. Cards is read-only for these values; Inspector is the single editing surface for title, Synopsis, and metadata.
- **CARD-007:** Explorer and Inspector remain available around Cards.
- **CARD-008:** Double-clicking a document Card switches to Editor and opens it. A single click on a group Card selects and expands or collapses that group; a single click on a document Card selects it without narrowing to a subtree.
- **CARD-009:** Cards and Explorer share multi-selection and applicable move/copy/cut behavior.
- **CARD-010:** A Status value appears only when the project defines and exposes that field.
