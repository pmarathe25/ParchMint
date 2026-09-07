# Release packaging

Build the desktop package on each target OS: Windows MSI, macOS DMG, or Linux DEB.
The CI release job does this after the workspace checks pass. Version-tag pushes
publish the installers and SHA-256 files to
[GitHub Releases](https://github.com/pmarathe25/ParchMint/releases).
See the [installation guide](../docs/install.md) for user instructions.

## Publish a release

Set `workspace.package.version` in `Cargo.toml`, update `Cargo.lock`, and commit
the release changes. Push an annotated tag that matches the version:

```console
git tag -a v0.1.0 -m "ParchMint 0.1.0"
git push origin main v0.1.0
```

Use the version being released in both commands. The workflow publishes only
after all workspace checks and all three platform packages succeed. It checks
the tag against `Cargo.toml`, verifies package checksums, and attaches the
packages to a release with generated notes and an installation link.

Use a numeric `major.minor.patch` version; Windows Installer limits those fields
to 255, 255, and 65535. Prerelease and build suffixes are rejected so package
versions and upgrade ordering stay consistent across platforms.

Branch builds, pull requests, and manual runs keep their packages as Actions
artifacts. Existing releases are not overwritten. The publishing job uses the
repository's `GITHUB_TOKEN` with `contents: write`; no additional secret is needed.

## Build and package

Use Python 3.11 or later, the pinned Rust toolchain, and these host tools:

- Linux: `dpkg-dev` (`sudo apt install dpkg-dev`).
- macOS: the system `hdiutil` tool.
- Windows: .NET 8 SDK and WiX 5.0.2:
  `dotnet tool install --global wix --version 5.0.2`.

Build and package on the target OS:

```console
cargo build --release --locked -j 1 -p parchmint-desktop --bin parchmint
cargo fetch --locked
python -m unittest discover -s packaging -p 'test_*.py'
python packaging/package.py --binary target/release/parchmint --platform linux --output release
```

Use `--platform macos` on macOS. On Windows, link the C runtime statically so
users do not need a separate Visual C++ redistributable. In PowerShell:

```powershell
$env:RUSTFLAGS = "-C target-feature=+crt-static"
cargo build --release --locked -j 1 --target x86_64-pc-windows-msvc -p parchmint-desktop --bin parchmint
python packaging/package.py --binary target/x86_64-pc-windows-msvc/release/parchmint.exe --platform windows --output release
```

Architecture defaults to the build host;
`--architecture` can identify a compatible executable built for another target.
The packager executes `--version` and rejects a binary that differs from Cargo.toml.

Each installed application includes the executable, application license,
bundled-font licenses, installation and user guides, and a dependency inventory with source-provided
license notices.
The inventory covers the locked workspace, including build and test tools; it
does not add their executables to the package. The packaging step reads cached
Cargo metadata as UTF-8 on every OS, so fetch the locked sources first. macOS
receives a `ParchMint.app` bundle and an Applications shortcut in the disk image;
notices stay inside the app's `Contents/Resources`. Windows installs into Program
Files with a Start menu shortcut and supports upgrades and uninstallation through
Windows Installer. Keep the `UpgradeCode` in `windows/package.wxs` stable.
Linux installs `/usr/bin/parchmint`, an application menu entry, and notices under
`/usr/share/doc/parchmint`. `dpkg-shlibdeps` derives linked dependencies; the
packager also declares the windowing libraries loaded at runtime.
A SHA-256 sidecar checks the package bytes. `source.json` records the Git revision
and whether the source tree had local changes. Existing outputs are refused.

## Release build isolation

Build `parchmint-desktop` alone. A workspace build unifies features with test
packages, which can enable the interaction harness and detailed diagnostics.
Normal startup disables observation collection. Default release diagnostics
record only warnings and errors; `--no-default-features` omits the logger.
CI tests the release filter separately from the harness build.

CI builds Linux packages on Ubuntu 24.04, tests Linux and Windows installation
and removal, and mounts the macOS image to run its bundled executable.

Packages are unsigned. Signing and macOS notarization need platform credentials
and are not performed by this packager. Keep the matching source revision
available when distributing a build; the package version comes from Cargo.toml.
