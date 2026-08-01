# S40 — Project Repository, Save, and Recovery

## Goal

Make current authored data safe before derived backends and broad UI integration.

## Tasks

- Repository/codec/atomic-writer/durability adapters.
- Create/open/lazy-load.
- One-writer project lock and process project-session routing.
- Revisioned save queue, dirty resources, atomic multi-file state machine.
- Recovery journal/replay.
- Save acknowledgements/errors.
- Project-undo and composite-operation save semantics.
- Fault harnesses for termination, partial write, disk full/permission, stale lock.
- Readability without history/index/cache/recovery.

## Pass criteria

- No partial canonical state after failures.
- Acknowledged edits not lost.
- Recovery restores pending edits.
- Composite project operations do not persist partially.
- Recovery/migration/whole restore resets interactive undo as specified.
- No filesystem/serialization work on UI thread.
