# parchmint-index

> Read when changing FTS schema, search queries, indexed metadata, cached
> counts, rebuilds, or index compatibility.

Owns the disposable SQLite projection. Canonical files remain authoritative;
corruption or schema mismatch triggers rebuild. See
[Application services](../../docs/architecture/services.md) and
[ADR-0007](../../docs/architecture/adr/0007-sqlite-fts-cache.md).

Check: `cargo test -p parchmint-index`.
