# Spellcheck

- **SPELL-001:** Spellcheck is required in v1 on Windows, macOS, and Linux and must provide correct, performant behavior in every supported desktop runtime.
- **SPELL-002:** Spellcheck uses the project-default language and supports a global dictionary, project dictionary, token-level suggestions, and viewport/recent-change-bounded checking.
- **SPELL-003:** The v1 spellcheck language is `en-US` on every supported platform. New Project does not expose a language selector, and per-document language overrides remain deferred.
- **SPELL-004:** Spellcheck failure must not block typing or saving. Errors remain visible and recoverable; release is blocked until cross-platform correctness and latency gates pass or this specification is updated by the product owner.
- **SPELL-005:** A misspelled word is decorated in place, and its spelling context menu is anchored to that word with ranked spelling suggestions and applicable project/global dictionary actions.
- **SPELL-006:** Runtime-provided spellcheck may be disabled when the selected ParchMint spellcheck implementation would otherwise produce duplicate or inconsistent decorations or menus.
