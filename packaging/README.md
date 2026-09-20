# Release packaging

**Purpose:** Build Windows MSI, macOS DMG, and Linux DEB packages and publish
version-tagged builds to [GitHub Releases](https://github.com/pmarathe25/ParchMint/releases).
See [installation](../docs/install.md) for supported downloads and user instructions.

## Publish a release

Set `workspace.package.version` in `Cargo.toml`, update `Cargo.lock`, and commit.
Use a numeric `major.minor.patch` version, with fields no greater than 255, 255,
and 65535. The packager rejects prerelease and build suffixes to preserve the
same version and upgrade ordering across platforms.

Push an annotated tag matching that version. For version 0.1.1:

```console
git tag -a v0.1.1 -m "ParchMint 0.1.1"
git push origin main v0.1.1
```

[CI](../.github/workflows/ci.yml) publishes after workspace checks and all three
packages pass. It checks the tag against Cargo.toml, verifies checksums, and
creates a release with installers, SHA-256 files, generated notes, and an
installation link. Branch, pull-request, and manual builds retain Actions
artifacts. Existing releases are not overwritten. On a rerun, CI accepts an
existing release only when it targets the same commit and contains every
expected installer and SHA-256 file; an incomplete or inconsistent release
fails validation.

The publish job uses `GITHUB_TOKEN` with `contents: write`. macOS packages receive
an ad hoc signature for bundle integrity. This does not establish publisher trust
with Gatekeeper; first launch requires approval in macOS Privacy & Security.
To enable Developer ID signing and notarization for tagged releases, configure these Actions
secrets: `MACOS_CERTIFICATE_P12_BASE64`, `MACOS_CERTIFICATE_PASSWORD`,
`MACOS_SIGNING_IDENTITY`, `MACOS_APPLE_ID`, `MACOS_APP_PASSWORD` (an app-specific
password), and `MACOS_TEAM_ID`. CI imports the certificate into a temporary
keychain and removes it after packaging. No credentials selects ad hoc signing;
an incomplete configuration stops publication.

For a local notarized distribution build,
set `PARCHMINT_MACOS_SIGNING_IDENTITY` to your installed Developer ID identity,
`PARCHMINT_MACOS_NOTARY_PROFILE` to a `notarytool` keychain profile, and
`PARCHMINT_REQUIRE_NOTARIZATION=1`. The packager signs the completed app bundle,
verifies it, creates and verifies the DMG, signs and notarizes it, and staples the
ticket before calculating its checksum. See [Apple's distribution guide](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution).

The Windows executable, application windows, and all native packages use the
icons in `packaging/icons`, converted from the supplied ParchMint artwork.

## Prepare the host

Use Python 3.11 or later, the pinned Rust toolchain, and the target OS:

| Host | Packaging tool |
| --- | --- |
| Linux | `dpkg-dev`: `sudo apt install dpkg-dev` |
| macOS | System `hdiutil` |
| Windows | .NET 8 SDK and `dotnet tool install --global wix --version 5.0.2` |

Fetch sources for the dependency notices before packaging:

```console
cargo fetch --locked
```

## Build a package

On Linux:

```console
cargo build --release --locked -j 1 -p parchmint-desktop --bin parchmint
python3 -m unittest discover -s packaging -p 'test_*.py'
python3 packaging/package.py --binary target/release/parchmint --platform linux --output release
```

On macOS, use the same commands with `--platform macos`.
For Windows, use a fresh PowerShell session and link the C runtime statically so
users need no separate Visual C++ redistributable:

```powershell
$env:RUSTFLAGS = "-C target-feature=+crt-static"
cargo build --release --locked -j 1 --target x86_64-pc-windows-msvc -p parchmint-desktop --bin parchmint
Remove-Item Env:RUSTFLAGS
python -m unittest discover -s packaging -p 'test_*.py'
python packaging/package.py --binary target/x86_64-pc-windows-msvc/release/parchmint.exe --platform windows --output release
```

Build the desktop crate alone: workspace feature unification can enable harness
code and detailed diagnostics. Normal release diagnostics record warnings and
errors; `--no-default-features` omits logging.

Architecture defaults to the host. `--architecture` labels a compatible executable
built for another target. The packager runs `--version` and rejects a binary
that differs from Cargo.toml. Existing package or checksum outputs are refused.

## Package contents and checks

Packages include the executable, application and font licenses, installation and
user guides, and locked dependency notices. The inventory includes build and test
dependencies without installing their executables. Cached Cargo metadata is read
as UTF-8 on every OS. `source.json` records the source revision, version, and dirty
state; keep that source revision available when distributing the package.

| Package | Installed layout and integration |
| --- | --- |
| MSI | Program Files and Start menu shortcut; Windows Installer upgrades and removal |
| DMG | App bundle and Applications shortcut; notices in `Contents/Resources` |
| DEB | `/usr/bin/parchmint`, application menu entry, and `/usr/share/doc/parchmint` |

Keep the MSI `UpgradeCode` in [package.wxs](windows/package.wxs) stable.
`dpkg-shlibdeps` derives Linux linked dependencies; the packager adds windowing
libraries loaded at runtime. CI builds Linux on Ubuntu 24.04.

Each package has a SHA-256 sidecar. CI tests release log filtering and Linux and
Windows installation/removal. On an Apple silicon macOS runner,
`verify_macos.py` mounts the DMG, copies the app to a temporary Applications
directory, and ejects the image. It verifies the installed signature, architecture,
system-only dependencies, resources, and version, then captures the native editor
in both themes and Overview. The dark editor launch uses LaunchServices, as Finder
does. These captures are retained as Actions artifacts. This verifies the installed
application; an ad hoc signature still requires the user's Gatekeeper approval.
