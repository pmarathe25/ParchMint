# S90 — Foundation Integration

## Goal

Integrate persistence, design shell, production editor, spellcheck, history, and search into one working foundation before feature waves.

## Entry conditions

Accepted handoffs for S40, S50, S60, S65, S70, and S80.

## Tasks

- Wire real create/open/save/recovery into the approved shell.
- Mount editor and shared toolbar; connect revision/error/save state.
- Integrate project commands/undo with editor project-operation boundaries.
- Integrate history/search/spellcheck through their ports.
- Integrate System/Light/Dark across real windows with no project mutations.
- Integrate one-process/multiple-project-window routing and project locks.
- Run cross-platform end-to-end foundation flows and applicable Tier B gates.
- Re-run generated contract/token drift checks.

## Pass criteria

- Create/open/edit/save/recover works through real services.
- Two-view/editor/spellcheck/search/history/project undo coexist without state-owner conflicts.
- Appearance updates every open window and leaves canonical files unchanged.
- No UI-thread I/O/analysis regressions.
- Windows/macOS/Linux foundation smoke passes.
- No unresolved material deviation.
