# Repository guidelines

**Purpose:** Map contributor instructions to the code being changed. Read the
relevant linked documentation before editing. Tests define supported behavior.

## Start here

- [README.md](README.md): application overview and build, run, test, lint, and
  format commands.
- [Documentation index](docs/README.md): guides by task.
- [User guide](docs/user-guide.md): application workflows.
- [Architecture](docs/architecture/architecture.md): flow, ownership, and crate
  boundaries.

## Read the component guide

Read a crate's `README.md` before changing it. Each guide defines its role,
public contract, and implementation notes. Use nearby unit and contract tests
for behavior; contract modules use the `*_contract_tests.rs` suffix.

- Before changing headless flows, read the
  [UI driver guide](tests/parchmint-ui-driver/README.md).
- Before changing captures, comparisons, or references, read
  [visual verification](tests/parchmint-ui-verification/README.md).
- For shared fixtures and native test controls, read
  [test support](tests/parchmint-test-support/README.md).
- For native installers, read [packaging](packaging/README.md).
- Before editing `third_party/`, inspect the patched renderer's history and
  consumers.

## Build constraints

Use the pinned Rust toolchain and locked Cargo commands. This workstation has
limited memory: compile with one job and never run compilation commands in
parallel.
