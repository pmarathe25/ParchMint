# ParchMint

ParchMint is a local-first desktop application for writing and organizing
novels on Windows, macOS, and Linux. Projects stay in ordinary files on your
computer, and the application keeps save history without requiring an online
account.

## Run from source

Install the Rust toolchain listed in `rust-toolchain.toml`, then run:

```console
cargo run --locked -p parchmint-desktop --bin parchmint
```

To build an optimized executable, run:

```console
cargo build --release --locked -p parchmint-desktop --bin parchmint
```

The executable is written to `target/release/parchmint` on macOS and Linux or
`target/release/parchmint.exe` on Windows.

## Documentation

- [User guide](docs/user-guide.md) explains projects, writing, search, History,
  recovery, settings, and export.
- [Architecture](docs/architecture/architecture.md) explains crate boundaries,
  data ownership, and background work.
- [UI design](docs/ui-design/README.md) records the visual language and screen
  composition.
- [Documentation index](docs/README.md) links the detailed contributor
  references.

Tests define supported behavior. Crate-level `README.md` files document the
interfaces and implementation details for each component.

## Development

```console
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
```

## License

ParchMint is free software licensed under the
[GNU General Public License, version 3 or later](LICENSE).
