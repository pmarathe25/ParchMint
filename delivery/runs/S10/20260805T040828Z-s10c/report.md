# S10 Dark Dual-Editor Recapture Report

**Stage:** S10
**Run:** 20260805T040828Z-s10c
**Run role:** repair
**Result:** needs approval (target changed during repair)
**Baseline commit:** `3d5d097cdb75b46e9b4409ca5f914b710a0190f8`
**Candidate commit:** blank (Orchestrator finalization)
**Output commit:** blank (Orchestrator finalization)

## Result

The completed Dark recapture remains preserved in `evidence/recapture.json` and
`evidence/exports/`. A subsequent bounded read-only comparison inspected both
live boards. They are not semantically redundant: `editor-dual-two-manuscript`
shows Chapter One and Chapter Two in separate panes, while
`editor-same-document-two-views` shows Chapter One in both panes with explicit
12%/68% scroll and independent selection annotations. Both structurally include
Manuscript and Research roots and the Harbor Notes row. In the rendered
two-Manuscript board, however, Research and Harbor Notes are absent because the
prior repair left those two Explorer shapes hidden along with the two obsolete
Harbor Notes pane tabs. This conflicts with TREE-001. The changed target now
requires product-owner approval before canonical integration.

## Evidence and limits

See `evidence/board-comparison.json` for exact board IDs, pane identities,
state annotations, requirement mapping, and the rendered-versus-structural
Research finding. No Penpot shape, component, token, page, metadata, name, or
content was mutated. The same-document export attempt returned no image and no
replacement was selected.
