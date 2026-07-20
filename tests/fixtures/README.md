# Fixture catalog

Fixtures are grouped by contract rather than by test implementation:

- `markdown/` contains focused front matter, extension, pairwise, malformed,
  Unicode, and repeated-save inputs.
- `recovery/` contains format-1 and newer-version journal records.
- `migration/` records the current format-1 no-op chain.
- `corpus/` contains deterministic seed/configuration manifests only.
- `spike/` retains the original architecture spike inputs.

Keep fixtures short enough to review. A fixture that exposes a product defect
must get a named regression test and a handoff note; do not weaken expected
behavior to make it pass.
