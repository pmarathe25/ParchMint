# S110 — System Integration and Validation

## Goal

Validate the complete integrated v1 application and dispatch bounded repairs that do not require G20.

## Tasks

- Complete requirement traceability.
- Run all Tier A suites.
- Run required Tier B native checks for every affected capability.
- Run scheduled/release Tier C workloads: full 250k one/two-view matrix, exact 20M corpus, 1M checkpoints, extended IME/accessibility/high-DPI/memory/fault/interchange.
- Validate project undo/global replacement/save/history consistency.
- Validate spellcheck language/dictionaries/suggestions/menu/performance on all platforms.
- Validate System/Light/Dark propagation, contrast, references, and zero canonical/history effects.
- Validate no search subtree/scope control, no per-document spellcheck language, and no deferred aggregate word counts.
- Run visual comparisons in both themes.
- Dispatch one bounded repair per defect where contracts/scope remain valid.

## Pass criteria

Every requirement is pass or blocked for a G20/current-spec change; no unknown native result; no unexplained major design deviation; all mandatory Tier C release evidence is available or explicitly scheduled for S130 candidate rerun.
