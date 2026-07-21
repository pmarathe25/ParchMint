# ADR-0014: incremental transactions, derived-state revisions, and model deltas

Status: Accepted

## Context

The version 1 canonical format must remain Markdown/TOML, but cloning and fully
validating a project for every command, rewriting every document on structural
save, rebuilding SQLite synchronously, and resetting the Qt model make the
10,000-node contract impossible. A crash between several individually atomic
file replacements also needs a project-level recovery rule.

## Decision

- Domain commands mutate one `Project` in place after command-local validation.
  Each command returns its bounded inverse and typed events. Event-local
  validation runs before success; the explicit full validator remains mandatory
  for open, migration, support validation, and equivalence tests.
- Storage maps events to a resource-shaped dirty set: manifests, outline,
  styles/presets, attachment catalog, document IDs, and tombstone IDs. Document
  locations are computed once per affected subtree and retained, rather than
  rediscovering ancestry for every file.
- A small canonical write set is one recoverable transaction. Before the first
  mutation, old bytes and a versioned record are durably renamed to
  `.parchmint/pending-save-v1`. Any failure, or the next open after interruption,
  restores every old file and removes newly created files. The pending record is
  deleted and its parent synced only after every replacement/deletion succeeds.
- The project owner freezes that write set before publishing a command delta,
  then a single serial project-save worker performs serialization, flushing,
  backup publication, and replacement. Owner location state is optimistic but
  retains a bounded command rollback; a failed write rolls back that command
  and every later queued command before one authoritative model reset. Close,
  navigation, and document canonical saves wait for the queue, so structural
  and document writers never race the same Markdown file.
- Content, structure, selection, and presentation revisions advance
  independently. Background index/count work carries an index revision; stale
  batches are ignored and a canonical mutation cancels/restarts an unfinished
  open-time scan from cheap immutable body handles.
- The outline projection emits stable-ID-derived insert, remove, move, and data
  deltas. Reset is reserved for an explicit filter/sort/focus projection change.
  The C++ model caches all roles from one bounded row payload.
- Editor changes cross the FFI boundary as a UTF-16 range, inserted fragment,
  and affected block range. Rust owns the full live body. Word/character deltas
  propagate through cached ancestors; semantic Markdown parsing occurs at a
  semantic operation or persistence boundary.
- Compile traversal and graph validation are iterative. Compile snapshots share
  immutable canonical body buffers. Markdown parsing, block rendering, asset
  packaging, CRC, and archive emission observe a cooperative cancellation token.
  Combined Markdown/plain-text export writes one block at a time to the atomic
  temporary artifact; EPUB/DOCX use checked ZIP64 store output.

## Consequences

Normal commands no longer provide an implicit full-project validation pass.
Consequently, every command/event mapping requires focused invariant tests plus
comparison against the explicit validator. The transaction record may briefly
duplicate the old bytes of the small changed set, but never the entire project.
Derived UI/cache state may be visibly indexing and incomplete; it cannot block
canonical editing or masquerade as complete totals.

This changes no canonical Markdown, TOML, recovery-journal, or asset format.
`.parchmint/pending-save-v1` is disposable operational recovery state, not a new
authoritative project format.
