# Packaging definitions

This directory contains the native package definitions for Windows, macOS, and
Linux. The templates retain `@NAME@` placeholders for values supplied by the
platform packaging job.

The package source executable is `target/release/parchmint` on macOS and Linux
and `target/release/parchmint.exe` on Windows. Build it from the locked source
tree with:

```text
cargo build --release --locked --package parchmint-desktop
```

The package templates already refer to this executable name. Generated packages
stay outside the source tree.
