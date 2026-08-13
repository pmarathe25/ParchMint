# Appearance

- **APPR-001:** Application settings must provide exactly three appearance choices in v1: `System`, `Light`, and `Dark`.
- **APPR-002:** `System` is the default and follows the current operating-system appearance while ParchMint is running.
- **APPR-003:** An explicit Light or Dark choice persists as an application preference and overrides later operating-system changes until changed by the user.
- **APPR-004:** Changing appearance updates every open ParchMint window without restarting and without entering project undo, project save, or project history.
- **APPR-005:** Dark appearance must use fully dark surfaces for the application, sidebar, Inspector, toolbar, editor chrome, and manuscript canvas, including the prose canvas.
- **APPR-006:** Light and Dark use the same semantic component and layout contracts. Production components must use semantic tokens and must not hard-code theme-dependent colors.
- **APPR-007:** Authored project styles, canonical HTML/CSS, and export output must not change when application appearance changes.
- **APPR-008:** Focus, selection, disabled, warning, error, comment, search-match, and save states must remain distinguishable in both appearances without relying on color alone.
- **APPR-009:** v1 provides Appearance through Settings. Toolbar and status-bar quick toggles are deferred.
