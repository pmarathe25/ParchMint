# ParchMint

**Purpose:** Write and organize novels on Windows, macOS, and Linux. ParchMint
keeps projects in ordinary local files and records save history without an
online account.

## Install

Download a Windows MSI, macOS DMG, or Linux DEB from
[GitHub Releases](https://github.com/pmarathe25/ParchMint/releases).
The [installation guide](docs/install.md) covers setup, updates, and removal.
Rust is needed only to build from source.

## Run from source

Use the pinned toolchain in [rust-toolchain.toml](rust-toolchain.toml):

```console
cargo run --locked -j 1 -p parchmint-desktop --bin parchmint
```

Build an optimized executable with:

```console
cargo build --release --locked -j 1 -p parchmint-desktop --bin parchmint
```

Output is `target/release/parchmint` on macOS and Linux or
`target/release/parchmint.exe` on Windows.
[Packaging](packaging/README.md) covers native installers and release builds.

## Development

Run compilation commands one at a time with one job on memory-limited machines:

```console
cargo check --workspace --all-targets --locked -j 1
cargo test --workspace --locked -j 1
cargo clippy --workspace --all-targets --locked -j 1 -- -D warnings
cargo fmt --all --check
```

## Documentation

- **Use ParchMint:** [User guide](docs/user-guide.md).
- **Understand the system:** [Architecture](docs/architecture/architecture.md).
- **Change a component:** [Documentation index](docs/README.md) and component READMEs.

Tests define supported behavior. [AGENTS.md](AGENTS.md) maps contributor instructions.

## License

ParchMint is licensed under the [GNU GPL, version 3 or later](LICENSE).
