# ParchMint implementation routes

`crates/` contains work owned by one documented crate. `integration/` contains the six stages that join crates or own workspace, CI, or release logistics. Architecture pages define APIs and boundaries; these pages define delivery order and the tests that retire a stage's specific risk.

## Sequence

1. [Bootstrap CI and supply chain](integration/01-bootstrap-ci-and-supply-chain.md) starts the workspace.
2. [Contracts](crates/02-contracts.md) and [domain](crates/03-domain.md) may proceed in parallel after bootstrap. [Project format](crates/04-project-format.md) follows both, then [test-support fixtures](crates/05-test-support-fixtures.md) follows contracts, domain, and format.
3. [Project repository](crates/06-project-repository.md) follows domain and format, and [project filesystem](crates/07-project-fs.md) follows the repository and fixtures. [History API](crates/08-history-api.md), [recovery API](crates/10-recovery-api.md), [search API](crates/13-search-api.md), and [export API](crates/15-export-api.md) may proceed in parallel once their listed prerequisites are complete.
4. Their implementations follow their APIs: [History Git2](crates/09-history-git2.md), [recovery filesystem](crates/12-recovery-fs.md), [Search SQLite](crates/14-search-sqlite.md), and [HTML export](crates/16-export-html.md). [Save](crates/11-save.md) follows [Project format](crates/04-project-format.md), [Project repository](crates/06-project-repository.md), [History API](crates/08-history-api.md), and [Recovery API](crates/10-recovery-api.md).
5. [Editor API](crates/17-editor-api.md), [spellcheck contract](crates/18-spellcheck-api-contract.md), and [service test support](crates/19-test-support-services.md) establish their seams. [Application](crates/20-application.md) and [core CLI](crates/21-core-cli.md) then lead to [headless backend integration](integration/22-headless-backend-integration.md).
6. [Design system](crates/23-design-system.md), [preferences](crates/24-preferences.md), [workspace state](crates/25-workspace-state.md), [platform API](crates/26-platform-api.md), [platform native](crates/27-platform-native.md), [UI API](crates/28-ui-api.md), [UI Iced shell](crates/29-ui-iced-shell.md), and [desktop](crates/30-desktop.md) proceed as their direct prerequisites become ready.
7. [Editor core](crates/31-editor-core.md) can proceed alongside the desktop foundation until [editor feasibility](integration/32-editor-feasibility.md) requires both. Feasibility must pass before [editor Iced](crates/33-editor-iced.md). Then complete [editor/save/recovery integration](integration/34-editor-save-recovery-integration.md), [spellcheck implementation](crates/35-spellcheck-en-us.md), [UI Iced editor](crates/36-ui-iced-editor.md), and [UI Iced project features](crates/37-ui-iced-project-features.md).
8. [Complete application](integration/38-complete-application.md) precedes [native packaging and release](integration/39-native-packaging-and-release.md).

Parallel work shares no crate, generated output, lockfile, or CI configuration. The numbered dependencies above take precedence over a potential parallel run.

## Crate coverage

| Crate | Owning stages |
| --- | --- |
| `parchmint-contracts` | [02](crates/02-contracts.md) |
| `parchmint-domain` | [03](crates/03-domain.md) |
| `parchmint-project-format` | [04](crates/04-project-format.md) |
| `parchmint-test-support` | [05](crates/05-test-support-fixtures.md), [19](crates/19-test-support-services.md) |
| `parchmint-project-repository` | [06](crates/06-project-repository.md) |
| `parchmint-project-fs` | [07](crates/07-project-fs.md) |
| `parchmint-history-api` | [08](crates/08-history-api.md) |
| `parchmint-history-git2` | [09](crates/09-history-git2.md) |
| `parchmint-recovery-api` | [10](crates/10-recovery-api.md) |
| `parchmint-save` | [11](crates/11-save.md) |
| `parchmint-recovery-fs` | [12](crates/12-recovery-fs.md) |
| `parchmint-search-api` | [13](crates/13-search-api.md) |
| `parchmint-search-sqlite` | [14](crates/14-search-sqlite.md) |
| `parchmint-export-api` | [15](crates/15-export-api.md) |
| `parchmint-export-html` | [16](crates/16-export-html.md) |
| `parchmint-editor-api` | [17](crates/17-editor-api.md) |
| `parchmint-spellcheck-api` | [18](crates/18-spellcheck-api-contract.md) |
| `parchmint-spellcheck-en-us` | [35](crates/35-spellcheck-en-us.md) |
| `parchmint-application` | [20](crates/20-application.md) |
| `parchmint-core-cli` | [21](crates/21-core-cli.md) |
| `parchmint-design-system` | [23](crates/23-design-system.md) |
| `parchmint-preferences` | [24](crates/24-preferences.md) |
| `parchmint-workspace-state` | [25](crates/25-workspace-state.md) |
| `parchmint-platform-api` | [26](crates/26-platform-api.md) |
| `parchmint-platform-native` | [27](crates/27-platform-native.md) |
| `parchmint-ui-api` | [28](crates/28-ui-api.md) |
| `parchmint-ui-iced` | [29](crates/29-ui-iced-shell.md), [36](crates/36-ui-iced-editor.md), [37](crates/37-ui-iced-project-features.md) |
| `parchmint-desktop` | [30](crates/30-desktop.md) |
| `parchmint-editor-core` | [31](crates/31-editor-core.md) |
| `parchmint-editor-iced` | [33](crates/33-editor-iced.md) |

## Integration and logistics stages

| Stage | Reason |
| --- | --- |
| [01](integration/01-bootstrap-ci-and-supply-chain.md) | Owns workspace, CI, and supply-chain policy rather than a crate. |
| [22](integration/22-headless-backend-integration.md) | Composes backend services through the real CLI. |
| [32](integration/32-editor-feasibility.md) | Measures the editor candidate across core and Iced boundaries. |
| [34](integration/34-editor-save-recovery-integration.md) | Proves revision hand-off across editor, save, and recovery. |
| [38](integration/38-complete-application.md) | Verifies complete cross-crate product flows. |
| [39](integration/39-native-packaging-and-release.md) | Owns packaging and release verification. |
