# Design Change Reconciliation

## Goal

Reconcile a new immutable approved Penpot handoff against the currently implemented handoff and generate bounded implementation tasks.

## Tasks

- Validate new manifest/checksums/governing versions.
- Compare tokens/themes/components/screens/references/interactions/accessibility/platform variants.
- Identify Light/Dark semantic-token changes and Appearance behavior impact.
- Map affected code/tests/fixtures/requirements/workstreams.
- Produce design-diff report, impact map, generated tasks, and pending approval.
- Pause only affected workstreams.

Do not maintain a permanent design-decision history. After approval, update current design/governing inputs and execute generated tasks.
