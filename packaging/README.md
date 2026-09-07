# Release packaging

Build the desktop package on each target OS, then create a portable archive.
The CI release job does this after the workspace checks pass. Artifacts remain
CI downloads; the workflow does not publish a release.

## Build and package

Use Python 3.11 or later and the pinned Rust toolchain:

```console
cargo build --release --locked -j 1 -p parchmint-desktop --bin parchmint
cargo fetch --locked
python -m unittest discover -s packaging -p 'test_*.py'
python packaging/package.py --binary target/release/parchmint --platform linux --output release
```

Use `--platform macos` on macOS. On Windows use `--platform windows` and the
`target/release/parchmint.exe` binary. Architecture defaults to the build host;
`--architecture` can identify a compatible executable built for another target.
The packager executes `--version` and rejects a binary that differs from Cargo.toml.

Each ZIP contains the executable, application license, bundled-font licenses,
user guide, and a dependency inventory with source-provided license notices.
The inventory covers the locked workspace, including build and test tools; it
does not add their executables to the archive. The packaging step reads cached
Cargo metadata, so fetch the locked sources first. macOS receives a `ParchMint.app` bundle. Linux receives an optional
desktop entry; install its executable on `PATH` before using that entry.
A SHA-256 sidecar checks the archive bytes. `source.json` records the Git revision
and whether the source tree had local changes. Existing outputs are refused.

## Release build isolation

Build `parchmint-desktop` alone. A workspace build unifies features with test
packages, which can enable the interaction harness and detailed diagnostics.
Normal startup disables observation collection. Default release diagnostics
record only warnings and errors; `--no-default-features` omits the logger.
CI tests the release filter separately from the harness build.

Archives are unsigned and use the host's native runtime dependencies. Signing,
macOS notarization, and installer distribution need platform credentials and
are not performed by this packager. Keep the matching source revision available
when distributing a build; the package version comes from Cargo.toml.
