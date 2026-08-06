# Scale and performance

## Required scale

- **PERF-001:** Projects up to 10–20 million words.
- **PERF-002:** Approximately 300–500 Manuscript documents and 25–50 Research documents.
- **PERF-003:** Individual documents up to approximately 250,000 words.
- **PERF-004:** All documents within the supported range receive the same features and interaction model.

## Interactive budgets

- **PERF-005:** Key-to-paint target: p95 ≤16 ms and p99 ≤33 ms under normal load.
- **PERF-006:** No save/history/search/spellcheck/export operation blocks the UI thread for more than 2 ms in one event-loop turn.
- **PERF-007:** Warm first editable viewport target is ≤250 ms for ordinary documents.
- **PERF-008:** At 250,000 words, the release gate is ≤1 second to first editable viewport on agreed reference hardware, with no feature reduction.
- **PERF-009:** Warm indexed global search begins returning results within 200 ms.
- **PERF-010:** Tree/Card movement visibly updates within 100 ms.
- **PERF-011:** Project open does not load every document body.
- **PERF-012:** Search rebuild, history maintenance, export, save, word-count rebuild, and spellcheck run in bounded background work and can be paused/cancelled where appropriate.
- **PERF-013:** Memory stabilizes under repeated open/edit/undo/search/close cycles; closing a document/view reclaims material editor resources.
- **PERF-014:** Transparent optimizations may be used only when selection, keyboard interaction, search, comments, clipboard, and undo semantics remain unchanged.
- **PERF-015:** Editor projection/recovery work must use bounded coalescing and must not accumulate an unbounded backlog during continuous typing.
