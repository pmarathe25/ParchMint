# Documentation

**Start here:** Choose a guide for your task. The architecture page maps the whole
system; component READMEs document their own interfaces. Tests define supported
behavior.

Keep current behavior in `docs/`, proposals in `plans/unimplemented/`, and brief
completed plans and findings in `plans/implemented/`. Keep raw benchmark output
outside the repository.

## Use ParchMint

- **Install or update:** [Installation guide](install.md).
- **Write and organize:** [User guide](user-guide.md).
- **Review proposed features:** [Future work](../plans/unimplemented/future-work.md).

## Develop and release

- **Build, test, and lint:** [Repository README](../README.md#development).
- **Understand ownership and find a crate:** [Architecture](architecture/architecture.md).
- **Run editor benchmarks:** [Benchmark guide](../plans/implemented/editor-benchmark-guide.md).
- **Follow repository instructions:** [AGENTS.md](../AGENTS.md).
- **Build and publish installers:** [Release packaging](../packaging/README.md).

## Verify changes

- **Create project fixtures:** [Test support](../tests/parchmint-test-support/README.md).
- **Exercise widgets and services:** [UI driver](../tests/parchmint-ui-driver/README.md).
- **Launch and use the native app:** [Isolated interactive review](../tests/parchmint-ui-driver/USABILITY.md#launch-and-use-the-native-application).
- **Judge complete UI tasks:** [Usability review](../tests/parchmint-ui-driver/USABILITY.md).
- **Review visuals and motion:** [Repository skill](../.agents/skills/parchmint-ui-review/SKILL.md).
- **Capture or compare images:** [Visual verification](../tests/parchmint-ui-verification/README.md).
