# `parchmint-core-cli`

## What it does

This crate provides a command-line interface for the ParchMint core. It runs
without opening a desktop window.

It parses commands, creates the required services, invokes application use
cases, prints safe results, and maps failures to stable exit codes. Domain rules
and direct project-file edits stay in their owning crates.

The CLI can create, open, validate, migrate, change, save, and recover projects.
It can also inspect History, search, rebuild search and other calculated data,
and export. These commands test the core services. Desktop behavior and
rich-text editing require tests in the desktop application.

## How it works

```text
arguments -> validate -> build real services -> run use case
                                            -> observations -> exit code
```

Normal commands construct the real service implementations. Mutating commands
use the same project lock and acknowledgement rules as the desktop application.

## Public API

```rust
pub enum CoreCommand {
    Create(CreateArgs),
    Open(OpenArgs),
    Validate(ValidateArgs),
    Migrate(MigrateArgs),
    Inspect(InspectArgs),
    RoundTrip(RoundTripArgs),
    Apply(ProjectCommandArgs),
    Undo(ProjectRef),
    Redo(ProjectRef),
    Save(ProjectRef),
    Recover(ProjectRef),
    Dictionary(DictionaryArgs),
    History(HistoryArgs),
    Search(SearchArgs),
    Rebuild(RebuildArgs),
    Export(ExportArgs),
}

pub trait ObservationSink {
    fn item(&mut self, item: CliObservation) -> Result<(), CliWriteError>;
    fn finish(&mut self, summary: CliSummary) -> Result<(), CliWriteError>;
}

pub struct CoreCli {
    services: CoreServices,
}

impl CoreCli {
    pub fn new(services: CoreServices) -> Self;

    pub async fn run(
        &self,
        command: CoreCommand,
        output: &mut dyn ObservationSink,
        cancel: CancellationToken,
    ) -> CliExit;
}

pub enum CliExit {
    Success,
    UsageError,
    UnsafeInput,
    Locked,
    InvalidProject,
    Cancelled,
    Failed,
}
```

Machine-readable JSON output uses the CLI schemas in `parchmint-contracts`.
Scripts rely on that output. Human-readable messages can change without a new
schema version.

## Implementation

```rust
async fn run(command: CoreCommand, sink: &mut dyn ObservationSink) -> CliExit {
    let request = match validate(command) {
        Ok(request) => request,
        Err(error) => return report_usage(error, sink),
    };

    let services = match CoreServices::open_real(request.scope()).await {
        Ok(services) => services,
        Err(error) => return report_failure(error, sink),
    };

    match services.execute(request, sink).await {
        Ok(summary) => finish(summary, sink),
        Err(error) => report_failure(error, sink),
    }
}
```

The CLI writes results in limited-size batches and supports cancellation. It
returns success after the requested operation actually finishes. For example, a
save command succeeds after the files and matching History checkpoint are safe.
By default, diagnostic output omits prose, search text, dictionary entries, and
full paths.

The CLI offers a fixed set of operations and accepts project-relative paths. It
does not accept raw SQL, Git commands, arbitrary filesystem changes, shell
commands, or network requests.
