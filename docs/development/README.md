# Developer guide

> Read for build and contribution tasks. Use the narrow page matching the change.

| Need | Read |
|---|---|
| Install tools and configure Qt | [Setup](setup.md) |
| Choose and run checks | [Testing](testing.md) |
| Change Rust, C++, QML, persistence, or async work | [Coding conventions](conventions.md) |
| Measure scale or latency | [Performance](performance.md) |
| Add or change user-visible text | [Localization](localization.md) |
| Write or restructure docs | [Documentation conventions](../conventions.md) |
| Understand ownership first | [Architecture](../architecture/README.md) |

Crate-level entry points live in `crates/*/README.md`. Persistent-format changes
also require the relevant [format specification](../format/) and may require a
new [ADR](../architecture/adr/README.md).
