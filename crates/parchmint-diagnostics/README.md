# `parchmint-diagnostics`

**Purpose:** Record bounded local diagnostics without changing application results.
Normal releases record warnings and errors. Debug builds and the `capture` feature
also retain traces, timing aggregates, and at most 4,096 events.

## Interface

`configure_file(data_directory)` opens `logs/parchmint-debug.log`.
`event!` checks the level before evaluating fields, avoiding formatting and
allocation for disabled traces. `event` accepts prepared fields. Callers log
operations and identifiers, never document text. See [lib.rs](src/lib.rs).

## Storage and overhead

A mutex serializes writes and rotation; the log stays within 1 MiB. The final
path rejects symlinks and Windows reparse points. Logging failures are ignored;
enabled events use standard error before configuration.

Warnings and errors write synchronously. Normal releases omit in-memory capture
and timing collection, so ordinary editing performs no diagnostic I/O. The
[release logging test](tests/release_logging.rs) verifies that filtered fields
are not evaluated. The desktop's `--no-default-features` build omits diagnostics.
