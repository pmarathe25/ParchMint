# `parchmint-search-sqlite`

**Purpose:** Implement [SearchIndex](../parchmint-search-api/README.md) with
bundled SQLite FTS5. Each open project has one worker and database connection.
SQL, connections, rows, statements, and SQLite errors remain private.

## Interface and storage

`SqliteSearchIndex::new` accepts a project directory. See [lib.rs](src/lib.rs)
for methods and [Cargo.toml](Cargo.toml) for the pinned `rusqlite` dependency.
Startup creates an FTS5 table and fails if FTS5 is unavailable.

The database is `.parchmint/cache/search.sqlite`, excluded from History and
rebuildable from current project text. Corruption, an incompatible schema, or a
failed integrity check triggers deletion and rebuilding.

## Query implementation

One transaction updates document content, the FTS row, and the revision. Rows
retain stable project, document, block, field, and revision IDs.
The tokenizer is `unicode61 remove_diacritics 2`.

The API accepts ordinary text and known field names. The implementation quotes
text instead of accepting raw SQL or FTS5 `MATCH` expressions. It checks FTS
candidates against exact case-sensitive and Unicode whole-word rules, then emits
small result batches.

Queries carry generation numbers. Cancellation interrupts SQLite when safe;
the application ignores older generations. All database work stays on the
background worker and requires no network access.
