# Project format

## Goal

Implement deterministic canonical project files and format migrations.

## Depends on

- [02 Contracts](02-contracts.md)
- [03 Domain](03-domain.md)

## Owning crate(s)

[`parchmint-project-format`](../../docs/architecture/crates/parchmint-project-format.md)

## Requirements and UI design

- [Canonical project data](../../docs/product/canonical-project-data.md)
- [Rich text and semantic styles](../../docs/product/rich-text-and-semantic-styles.md)
- [Comments and annotations](../../docs/product/comments-and-annotations.md)

## Work

- Encode and decode canonical HTML, TOML, CSS, text, and annotation sidecars.
- Enforce UTF-8, LF, stable serialization, safe relative paths, sanitization, and complete in-memory migrations.

## Stage-specific tests and validation

Run byte-identical round trips, invalid/unsafe-content rejection, Unicode and path portability fixtures, and migration fixtures that preserve stable IDs.
