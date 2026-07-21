# Concurrency and freshness

> Read when adding background work, revisions, cancellation, progress, or model publication.

Qt owns the event loop. Rust workers receive owned inputs and return typed
completions; workers never mutate QML or Qt models directly.

## Worker ownership

- Document worker: journal and canonical-body writes.
- Structural worker: manifests and multi-file transactions.
- Search worker: index rebuild and queries.
- Compile/export worker: immutable compile IR and prepared artifacts.
- Qt render step: native PDF text shaping.

## Freshness protocol

Every request carries a `WorkStamp`:

```text
project generation + narrow content/structure/style/index revision
```

Opening or replacing a project advances the generation. Mutations advance the
narrowest relevant revision. Immediately before publication or destination
commit, compare the completion stamp with the owner. Drop stale success.

## Rules

- Pass changed blocks, rows, counts, or artifacts—not whole projects per interaction.
- Check cancellation inside long scans, traversals, reads, and renderer loops.
- Treat closed channels and failed joins as typed errors.
- Close worker input and join its thread on drop.
- Publish immutable snapshots or typed model deltas on the UI owner.
- Do not acknowledge a save until the matching canonical revision commits.

Rationale: [ADR-0003](adr/0003-cxx-qt-boundaries-and-threading.md),
[ADR-0010](adr/0010-revisioned-document-lifecycle.md), and
[ADR-0014](adr/0014-incremental-transactions-and-revisions.md).
