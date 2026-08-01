# S40 — Project Repository, Save, and Recovery

## Goal

Make current authored data safe before history, search, and broad UI integration.

## Tasks

Implement Phase 2:

- `ProjectRepository`, `CanonicalCodec`, `AtomicWriter`, and platform durability adapters.
- Create/open/lazy-load workflows.
- One-writer project lock.
- Revisioned save queue and dirty-resource tracking.
- Atomic multi-file save transaction.
- Versioned recovery journal and replay.
- Save acknowledgements and error states.
- Forced-termination, partial-write, disk-full/permission, and stale-lock fault harnesses where feasible.
- Current-file readability when history, index, cache, and recovery directories are removed.

## Required outputs

- Stable persistence/recovery port implementations.
- Fault evidence on all three platforms.
- Handoff describing save/revision semantics for history, search, and editor agents.

## Pass criteria

- No partial canonical state after injected failures.
- Acknowledged edits are not lost.
- Recovery restores uncheckpointed edits after termination.
- Current project opens without derived state.
- No filesystem or serialization work runs on the UI thread.
