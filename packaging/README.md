# Packaging definitions

These templates describe the native package layouts. They intentionally retain
`@NAME@` inputs for values that current native CI has not proved or release
owners have not approved.

`release-inputs.toml` is the source of readiness state. Normal CI accepts an
explicit `missing` input with a reason. The release verifier requires every
input to be available, binds those artifact paths and signing-input package
digests to the candidate manifest, and then requires real, hash-bound evidence.
See `docs/release/README.md` for the release flow.

The package source executable is `target/release/parchmint` on macOS and Linux
and `target/release/parchmint.exe` on Windows. Build it from the locked source
tree with:

```text
cargo build --release --locked --package parchmint-desktop
```

The package templates already refer to this executable name. Building it does
not satisfy the release verifier's native install or launch evidence gates.
