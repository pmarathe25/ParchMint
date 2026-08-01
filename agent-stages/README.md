# ParchMint Agent Stages

The Orchestrator dispatches one stage instruction per isolated run.

Order:

1. S00 repository intake
2. S10 design reconciliation and G10
3. S20 bootstrap
4. S30 contracts/domain/project undo/format
5. S40 persistence/recovery
6. S50 design system/shell
7. S55 editor feasibility
8. S60 production editor foundation
9. S65 spellcheck foundation
10. S70 history and S80 search
11. S90 foundation integration
12. S100 generated feature waves
13. S110 integration validation
14. S120 release hardening
15. S130 independent validation and G90

S40, S50, and S55 may run in parallel after S30. S60 follows S55; S65 follows S60. S90 waits for S40/S50/S60/S65/S70/S80.

Stage agents do not merge themselves, change governing scope, or rely on chat history. Every run produces `dispatch.yaml`, `status.yaml`, `handoff.yaml`, `report.md`, and `evidence/`.
