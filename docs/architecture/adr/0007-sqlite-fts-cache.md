# ADR-0007: SQLite FTS5 is disposable derived state

Status: Accepted

## Decision

Use SQLite FTS5 with the Unicode tokenizer for search/statistics cache rows.
Bundle SQLite through Rust for a consistent FTS5 feature set. Store only derived
data and source correlation identifiers under `.parchmint/index.sqlite`.

Incremental update and deletion are transactional. Full rebuild drops rows and
repopulates exclusively from canonical Markdown/TOML. Removing the database must
never remove or alter authored data. Index work always runs off the UI thread and
stale results are rejected by generation/revision.
