# Install

> Read when installing a packaged release or building from source.

ParchMint targets Windows 10 22H2+, macOS 13+, and contemporary 64-bit Linux
distributions. Releases may provide MSI/ZIP, DMG/TGZ, and AppImage/Flatpak/TGZ
artifacts. Verify release checksums and signatures before installation.

Installing, upgrading, or uninstalling ParchMint does not remove project folders.

## Build from source

On Ubuntu 24.04 or newer, use the repository's native dependency installer:

```sh
git clone https://github.com/pmarathe25/ParchMint.git
cd ParchMint
./scripts/install-dependencies.sh
source scripts/host-env.sh
just bootstrap
just build
just run
```

On other platforms, install and configure the pinned tools using the upstream
links and platform notes in [developer setup](../development/setup.md).
`just bootstrap` verifies Rust, CMake, and Qt discovery. If Qt is outside the
normal search path, export the kit environment shown in the developer guide.
