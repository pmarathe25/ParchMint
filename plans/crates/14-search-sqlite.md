# Search SQLite

## Goal

Implement the disposable FTS5 index behind `SearchIndex`.

## Depends on

- [07 Project filesystem](07-project-fs.md)
- [13 Search API](13-search-api.md)

## Owning crate(s)

[`parchmint-search-sqlite`](../../docs/architecture/crates/parchmint-search-sqlite.md)

## Requirements and UI design

- [Search and replacement](../../docs/product/search-and-replacement.md)
- [Canonical project data](../../docs/product/canonical-project-data.md)

## Work

- Use the pinned bundled `rusqlite` selection specified by the crate page, one worker/connection per project, safe query construction, revisioned transactions, and rebuildable cache storage.

## Stage-specific tests and validation

Test FTS5 availability, user-text escaping, Unicode whole-word and case-sensitive filtering, cancellation, integrity failure rebuild, and warm-result bounds.
