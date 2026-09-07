# `parchmint-diagnostics`

This crate records local diagnostics without changing application results.
Normal release builds record warnings and errors. Debug builds and the explicit
`capture` feature also retain traces, timing aggregates, and up to 4,096 events.

## Interface

`configure_file(data_directory)` opens `logs/parchmint-debug.log`.
`event!` checks the level before evaluating fields, so disabled trace events
avoid string formatting and allocation. `event` accepts already prepared fields.
Callers record operations and identifiers; document text stays out of the log.

## Implementation

The file stays within 1 MiB and uses a mutex to serialize writes and rotation.
The final path is opened without following symlinks or Windows reparse points.
Failures are ignored; before configuration, enabled events go to standard error.
Warnings and errors write synchronously. Normal release builds omit in-memory
capture and timing collection, so ordinary editing performs no diagnostic I/O.
`--no-default-features` on the desktop omits diagnostics entirely.

See [lib.rs](src/lib.rs) for the logger and [release_logging.rs](tests/release_logging.rs)
for the check that filtered fields are not evaluated.
