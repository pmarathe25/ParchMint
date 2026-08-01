# S90 — Foundation Integration

## Goal

Integrate the accepted persistence, shell, editor, history, and search foundations into one coherent application baseline before feature slices.

## Tasks

1. Merge accepted stage branches in dependency order through an integration branch.
2. Resolve only mechanical conflicts; redispatch semantic conflicts.
3. Connect production application ports without duplicating state in React or ProseMirror.
4. Replace shell mocks with minimal real project/open/save/history/search/editor flows.
5. Run the complete contract, canonical-format, save/recovery, history, search, editor, visual-smoke, accessibility-smoke, and native launch suites.
6. Verify background work does not block the UI thread.
7. Produce an ownership map for feature-slice work.

## Required outputs

- Integrated foundation commit.
- Updated traceability matrix.
- Stable extension points and public contract versions.
- Integration handoff and file ownership map.

## Pass criteria

All foundation gates pass together on all three platforms and no selected adapter type leaks through public application/domain boundaries.
