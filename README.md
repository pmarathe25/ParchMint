# ParchMint

ParchMint is a local-first desktop application for writing and organizing
novels on Windows, macOS, and Linux. Projects stay in ordinary files on your
computer, and the application keeps save history without requiring an online
account.

## Install

Download the installer for your computer from
[GitHub Releases](https://github.com/pmarathe25/ParchMint/releases): Windows MSI,
macOS DMG, or Linux DEB. See the [installation guide](docs/install.md) for platform
instructions and updates. Rust is only needed when building from source.

## Run from source

Install the Rust toolchain listed in `rust-toolchain.toml`, then run:

```console
cargo run --locked -j 1 -p parchmint-desktop --bin parchmint
```

To build an optimized executable, run:

```console
cargo build --release --locked -j 1 -p parchmint-desktop --bin parchmint
```

The executable is written to `target/release/parchmint` on macOS and Linux or
`target/release/parchmint.exe` on Windows. See [packaging](packaging/README.md)
for native release packages.

## Documentation

- [User guide](docs/user-guide.md) explains projects, writing, search, History,
  recovery, settings, and export.
- [Architecture](docs/architecture/architecture.md) explains crate boundaries,
  data ownership, and background work.
- [Documentation index](docs/README.md) links the detailed contributor
  references.

Tests are the single source of truth for application requirements. Crate-level
`README.md` files document the interfaces and implementation details for each component.

## Development

```console
cargo check --workspace --all-targets --locked -j 1
cargo test --workspace --locked -j 1
cargo clippy --workspace --all-targets --locked -j 1 -- -D warnings
cargo fmt --all --check
```

## License

ParchMint is free software licensed under the
[GNU General Public License, version 3 or later](LICENSE).
