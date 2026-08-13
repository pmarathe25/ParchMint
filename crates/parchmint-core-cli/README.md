# `parchmint-core-cli`

## What it does

This crate provides a command-line interface for the ParchMint core. It runs
without opening a desktop window.

It parses commands, creates the required services, invokes application use
cases, prints safe results, and maps failures to stable exit codes. Domain rules
and direct project-file edits stay in their owning crates.

The CLI can create, open, validate, migrate, save, recover, checkpoint,
restore, and close projects; apply a no-op command; replace one document body
(`edit`); inspect or terminate a pending recovery journal; and run History,
index, search, rebuild, and export operations. These commands test the core
services. Desktop behavior and rich-text editing require tests in the desktop
application.

## How it works

```text
arguments -> parse (--machine, --cancel) -> run use case with real services
                                          -> emit message or machine JSON
                                          -> exit code
```

Normal commands construct the real service implementations. Mutating commands
use the same project lock and acknowledgement rules as the desktop application.

## Interface

The crate exports two synchronous entry points; `main.rs` calls `run_process`
and exits with its numeric status:

```rust
pub fn run_process() -> i32;
pub fn run_args(arguments: impl IntoIterator<Item = String>) -> i32;
```

Stable exit codes (the internal `Outcome` enum is private; its numeric values
are the contract):

```rust
Success = 0; Failed = 1; Usage = 2; UnsafeInput = 3;
Locked = 4; InvalidProject = 5; Cancelled = 6;
```

Arguments are a subcommand name followed by positional paths and text;
`--machine` prints one `CliOutputV1` JSON document to stdout, and `--cancel`
reports `Cancelled` (6) without executing the command:

```text
create <dir> | open <dir> | validate <dir> | migrate <dir> | inspect <dir>
edit <dir> <resource> <text> | command <dir> <operation> | terminate <dir>
save <dir> | close <dir> | recover <dir>
checkpoint <dir> <name> | restore <dir> <checkpoint> | history <dir>
index <dir> | search|query <dir> <text> | rebuild <dir>
export <dir> <output>
```

Machine-readable JSON output uses the CLI schemas in `parchmint-contracts`.
Scripts rely on that output. Human-readable messages can change without a new
schema version.

## Implementation

```rust
pub fn run_args(arguments: impl IntoIterator<Item = String>) -> i32 {
    let parsed = parse(arguments);
    let result = match parsed.command {
        Ok(_) if parsed.cancelled => CommandResult::from(Outcome::cancelled()),
        Ok(command) => execute(command),
        Err(outcome) => CommandResult::from(outcome),
    };
    emit(parsed.machine, result)
}
```

The CLI supports cancellation through `--cancel`, which reports `Cancelled`
without partial machine output. It returns success after the requested
operation actually finishes. For example, a save command succeeds after the
files and matching History checkpoint are safe. Default diagnostics print only
a fixed status message; machine output carries only safe summaries (such as
`checkpoint_id` or `hit_count`), never project paths, user prose, search text,
or dictionary entries.

The CLI offers a fixed set of operations and accepts project-relative paths.
It does not accept raw SQL, Git commands, arbitrary filesystem changes, shell
commands, or network requests.
