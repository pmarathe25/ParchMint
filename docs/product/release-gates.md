# v1 release gates

ParchMint v1 is complete only when:

1. Every must-level requirement is implemented or the current specification is explicitly updated by the product owner.
2. The maintained UI design documentation and `docs/ui-design/parchmint-ui.penpot`, approved by the product owner, have no unexplained major visual or interaction deviations in Light or Dark.
3. Canonical format golden tests and cross-platform round trips pass.
4. Save, recovery, project undo, history, corruption isolation, deletion, restoration, and composite global-replacement fault tests pass.
5. History, search, spellcheck, editor, persistence, export, and platform adapter contract tests pass.
6. Normal and 250,000-word document fixtures retain the same feature set in one and two views.
7. Performance budgets pass on agreed reference hardware or the specification is updated.
8. Clipboard, high-DPI, English spellcheck, and desktop interaction validation passes on Windows, macOS, and Linux. IME, international-text, screen-reader, and formal assistive-technology validation are not v1 release gates.
9. System/Light/Dark switching, persistence, contrast, and open-window propagation pass.
10. Installers/packages launch and operate on the supported platform matrix.
11. No required workflow depends on a proprietary project database, installed Git executable, network service, or raw source editing.
