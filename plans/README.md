# ParchMint implementation routes

Follow the shared [stage-delivery process](stage-delivery.md) for every stage.
The suggested subagent is the lowest tier expected to complete the whole stage
reliably; escalate when the actual scope or risk is greater than the plan shows.

| Stage | Suggested subagent |
| --- | --- |
| [01 — Bootstrap CI and supply chain](integration/01-bootstrap-ci-and-supply-chain.md) | complex_worker |
| [02 — Contracts](crates/02-contracts.md) | feature_worker |
| [03 — Domain](crates/03-domain.md) | complex_worker |
| [04 — Project format](crates/04-project-format.md) | feature_worker |
| [05 — Test-support fixtures](crates/05-test-support-fixtures.md) | fast_worker |
| [06 — Project repository](crates/06-project-repository.md) | feature_worker |
| [07 — Project filesystem](crates/07-project-fs.md) | complex_worker |
| [08 — History API](crates/08-history-api.md) | feature_worker |
| [09 — History Git2](crates/09-history-git2.md) | complex_worker |
| [10 — Recovery API](crates/10-recovery-api.md) | feature_worker |
| [11 — Save](crates/11-save.md) | complex_worker |
| [12 — Recovery filesystem](crates/12-recovery-fs.md) | complex_worker |
| [13 — Search API](crates/13-search-api.md) | feature_worker |
| [14 — Search SQLite](crates/14-search-sqlite.md) | complex_worker |
| [15 — Export API](crates/15-export-api.md) | feature_worker |
| [16 — Export HTML](crates/16-export-html.md) | feature_worker |
| [17 — Editor API](crates/17-editor-api.md) | feature_worker |
| [18 — Spellcheck API contract](crates/18-spellcheck-api-contract.md) | feature_worker |
| [19 — Test-support services](crates/19-test-support-services.md) | patch_worker |
| [20 — Application](crates/20-application.md) | complex_worker |
| [21 — Core CLI](crates/21-core-cli.md) | feature_worker |
| [22 — Headless backend integration](integration/22-headless-backend-integration.md) | complex_worker |
| [23 — Design system](crates/23-design-system.md) | feature_worker |
| [24 — Preferences](crates/24-preferences.md) | feature_worker |
| [25 — Workspace state](crates/25-workspace-state.md) | feature_worker |
| [26 — Platform API](crates/26-platform-api.md) | feature_worker |
| [27 — Platform native](crates/27-platform-native.md) | complex_worker |
| [28 — UI API](crates/28-ui-api.md) | feature_worker |
| [29 — UI Iced shell](crates/29-ui-iced-shell.md) | feature_worker |
| [30 — Desktop](crates/30-desktop.md) | feature_worker |
| [31 — Editor core](crates/31-editor-core.md) | complex_worker |
| [32 — Editor feasibility](integration/32-editor-feasibility.md) | complex_worker |
| [33 — Editor Iced](crates/33-editor-iced.md) | complex_worker |
| [34 — Editor save and recovery integration](integration/34-editor-save-recovery-integration.md) | complex_worker |
| [35 — Spellcheck engine evaluation and implementation](crates/35-spellcheck-en-us.md) | complex_worker |
| [36 — UI Iced editor](crates/36-ui-iced-editor.md) | complex_worker |
| [37 — UI Iced project features](crates/37-ui-iced-project-features.md) | complex_worker |
| [38 — Complete application](integration/38-complete-application.md) | complex_worker |
| [39 — Native packaging and release](integration/39-native-packaging-and-release.md) | complex_worker |
