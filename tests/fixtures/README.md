# Fixture catalog

Fixtures are grouped by contract rather than by test implementation:

- `markdown/` contains focused front matter, extension, pairwise, malformed,
  Unicode, and repeated-save inputs.
- `recovery/` contains format-1 and newer-version journal records.
- `migration/` records the current format-1 no-op chain.
- `corpus/` contains deterministic seed/configuration manifests only.
- `projects/` contains whole-project format fixtures (for example
  `format-edge-case`) that storage tests open read-only.
- `spike/` contains compact editor/Markdown compatibility inputs used by Qt
  adapter and parser regression tests.

Keep fixtures short enough to review. A fixture that exposes a product defect
must get a named regression test; do not weaken expected behavior to make it
pass.
