# Core CLI

## Goal

Expose real core operations without starting a desktop runtime.

## Depends on

- [09 History Git2](09-history-git2.md)
- [12 Recovery filesystem](12-recovery-fs.md)
- [14 Search SQLite](14-search-sqlite.md)
- [16 Export HTML](16-export-html.md)
- [20 Application](20-application.md)

## Owning crate(s)

[`parchmint-core-cli`](../../docs/architecture/crates/parchmint-core-cli.md)

## Requirements and UI design

- [Canonical user flows](../../docs/product/canonical-user-flows.md)
- [Privacy and security](../../docs/product/privacy-and-security.md)

## Work

- Add create, open, validate, migrate, inspect, command, save, recover, History, search, rebuild, and export commands with stable machine output and safe diagnostic redaction.

## Stage-specific tests and validation

Run CLI exit-code, cancellation, locked-project, machine-schema, and prose/path-redaction tests against real service implementations.
