# Application

## Goal

Coordinate project and document actions through the correct state owner.

## Depends on

- [06 Project repository](06-project-repository.md)
- [08 History API](08-history-api.md)
- [11 Save](11-save.md)
- [13 Search API](13-search-api.md)
- [15 Export API](15-export-api.md)
- [17 Editor API](17-editor-api.md)
- [18 Spellcheck API contract](18-spellcheck-api-contract.md)

## Owning crate(s)

[`parchmint-application`](../../docs/architecture/crates/parchmint-application.md)

## Requirements and UI design

- [Undo and redo](../../docs/product/undo-and-redo.md)
- [Explorer and hierarchy](../../docs/product/explorer-and-hierarchy.md)
- [Synopsis and metadata](../../docs/product/synopsis-and-metadata.md)
- [Search and replacement](../../docs/product/search-and-replacement.md)
- [Save, recovery, and closing](../../docs/product/save-recovery-and-closing.md)

## Work

- Implement `ProjectCommandDispatcher`, project undo, unopened-document operations, revisioned save requests, and atomic composite replacement.

## Stage-specific tests and validation

Test focus-selected undo domains, global replacement's one inverse/undo/checkpoint, failure rollback across open and closed documents, and undo reset after recovery, migration, or restore.
