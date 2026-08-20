# Core-state test simplification review

No high-confidence simplification preserves all unique coverage in the reviewed
domain, application, editor, save, and recovery tests.

- Editor queue sequences prove bounded coalescing; semantic, atomic, and comment
  cases cover different behavior.
- Recovery API replay cases are already table-driven across four isolation
  variants.
- Recovery filesystem tests separately cover truncation, checksum quarantine,
  compaction, intent persistence, and path safety.
- Application fixtures support distinct atomicity and lazy-state scenarios.
- Save tests separately cover concurrency, coalescing, priority, panic unwind,
  and post-commit History failure.

Small helpers such as `hash`, `batch`, and `replay_from` look similar across
modules, but consolidating them would add cross-module test infrastructure or
reduce local readability without removing meaningful test complexity.

This review used source inspection only. It did not run tests or change code.
