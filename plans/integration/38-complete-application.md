# Complete application

## Goal

Verify all required v1 flows in the integrated desktop application.

## Depends on

- [30 Desktop](../crates/30-desktop.md)
- [34 Editor save and recovery integration](34-editor-save-recovery-integration.md)
- [35 Spellcheck engine evaluation and implementation](../crates/35-spellcheck-en-us.md)
- [36 UI Iced editor](../crates/36-ui-iced-editor.md)
- [37 UI Iced project features](../crates/37-ui-iced-project-features.md)

## Owning paths

Cross-crate integration fixtures, platform runs, performance measurements, and visual acceptance checks.

## Requirements and UI design

- [Canonical user flows](../../docs/product/canonical-user-flows.md)
- [Release gates](../../docs/product/release-gates.md)
- [UI design index](../../docs/ui-design/README.md)

## Work

- Assemble all services and views into the complete Windows, macOS, and Linux application while retaining established crate boundaries.

## Stage-specific tests and validation

Run every canonical user flow, normal and 250,000-word one/two-view fixtures, performance budgets on agreed hardware, cross-platform canonical interchange, high-DPI/clipboard interaction checks, complete Light/Dark review, and recovery/history/search/spellcheck/export fault coverage.
