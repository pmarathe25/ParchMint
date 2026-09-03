# Repository Guidelines

Use this file as a map. Read the linked documentation for the part of the
repository you are changing. Tests are the authority for supported behavior.

## Start here

- [`README.md`](README.md) explains what ParchMint is and lists the main build,
  run, test, lint, and format commands.
- [`docs/README.md`](docs/README.md) is the documentation index.
- [`docs/user-guide.md`](docs/user-guide.md) explains how people use the
  application.
- [`docs/architecture/architecture.md`](docs/architecture/architecture.md)
  describes application flow, data ownership, and crate boundaries.
- [`docs/ui-design/README.md`](docs/ui-design/README.md) links the visual
  language, screen catalog, interaction patterns, platform conventions, and
  Penpot source.

## Find implementation details

- Read the `README.md` in the relevant directory under `crates/` before
  changing a component. It defines that crate's role, public contract, and
  implementation notes.
- Use nearby unit and contract tests for component behavior. Contract test
  modules use the `*_contract_tests.rs` suffix.
- Read [`tests/parchmint-ui-driver/README.md`](tests/parchmint-ui-driver/README.md)
  before changing headless end-to-end flows.
- Read [`tests/parchmint-ui-verification/README.md`](tests/parchmint-ui-verification/README.md)
  before changing UI captures, comparisons, or reference images.
- Shared fixtures and native test controls are described in
  [`tests/parchmint-test-support/README.md`](tests/parchmint-test-support/README.md).

## Other repository areas

- [`packaging/README.md`](packaging/README.md) covers native package inputs.
- `third_party/` contains the patched Iced renderer; inspect its history and
  consumers before editing it.

Use the pinned Rust toolchain and locked Cargo commands. This workstation has
limited memory, so run compilation commands with one job and do not run them
in parallel.
