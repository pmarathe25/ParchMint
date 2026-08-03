# S40 — Project Repository, Save, and Recovery

## Goal

Make current authored data safe before derived backends and broad UI integration.

## Tasks

- Repository/codec/atomic-writer/durability adapters.
- Create/open/lazy-load.
- One-writer project lock and process project-session routing.
- Schema-versioned atomic `ApplicationPreferenceStore` with compare-and-store generations for appearance/global dictionary.
- Revisioned save queue, dirty resources, immutable project/open-editor revision vectors, and atomic multi-file state machine.
- Hash-keyed idempotent checkpoint intents and reopen reconciliation across canonical replacement and History completion.
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
- A canonical revision vector receives its required checkpoint exactly once; mismatched project/editor revisions are never combined or reported Saved.
- Recovery/migration/whole restore resets interactive undo as specified.
- Preference restart, stale-generation, and write-failure tests pass without project mutation.
- No filesystem/serialization work on UI thread.
