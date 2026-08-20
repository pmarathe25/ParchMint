# Bug-finding review

## Confirmed bug: diagnostics startup can truncate a symlink target

`configure_file` follows an existing `logs/parchmint-debug.log` symlink, reads
the target's size, and opens that same path with `truncate(true)` when the size
is at least 1 MiB ([`crates/parchmint-diagnostics/src/lib.rs:96`](/home/pranav/Code/ParchMint/crates/parchmint-diagnostics/src/lib.rs:96)). `OpenOptions` follows symlinks, so this truncates the target file under the application's permissions.

**Trigger:** Create the application-data `logs` directory, replace
`parchmint-debug.log` with a symlink to an existing file that is at least 1
MiB, then start the desktop application. The startup diagnostics configuration
truncates the linked file before it writes the configuration event.

**Impact:** A local process or account able to modify ParchMint's data
directory can cause data loss in another writable file. The earlier
append-only implementation could write through a symlink, but the new bounded
logging path adds destructive truncation.

**Minimal fix:** Refuse a log path that is a symlink and open or replace the
log using no-follow, race-safe platform file APIs. Keep the existing file
descriptor for later rotation. Do not rely on a separate `symlink_metadata`
check alone, because the path can change before `open`.

**Regression test:** On platforms with symlink support, point the log path at
a 1 MiB sentinel file and assert that `configure_file` returns an error while
the sentinel bytes and length remain unchanged. Also cover the normal regular
file rotation path.

## Observation needing deeper investigation

The new task-outcome helpers persist every `Failed { message }` string as an
`error` diagnostics field ([`crates/parchmint-ui-iced/src/native.rs:725`](/home/pranav/Code/ParchMint/crates/parchmint-ui-iced/src/native.rs:725)). Those messages are constructed from generic adapter errors in
`ServiceFeedError::Service` ([`async_service_feeds.rs:1419`](/home/pranav/Code/ParchMint/crates/parchmint-ui-iced/src/async_service_feeds.rs:1419)); some lower layers include filesystem paths in their display output, for example
`FsError` ([`crates/parchmint-project-fs/src/lib.rs:48`](/home/pranav/Code/ParchMint/crates/parchmint-project-fs/src/lib.rs:48)). This conflicts with the diagnostics module's stated rule that callers record only operation names and non-content identifiers.

The reviewed sources establish path disclosure, but did not establish a
production error that includes document body text. Treat this as a privacy
hardening follow-up: log a stable error category and operation, and keep raw
messages only in the UI or in an explicitly reviewed local diagnostic channel.

## Evidence limits

This review inspected the current working tree, including the uncommitted
registry, diagnostics, worker-launch, and outcome changes. It did not run
builds, tests, Cargo metadata, or runtime traces, as required for the
low-memory review. No additional reproducible defect was confirmed from source
inspection.
