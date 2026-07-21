# Install

> Read when installing a packaged release or building from source.

ParchMint targets Windows 10 22H2+, macOS 13+, and contemporary 64-bit Linux
distributions. Releases may provide MSI/ZIP, DMG/TGZ, and AppImage/Flatpak/TGZ
artifacts. Verify release checksums and signatures before installation.

Installing, upgrading, or uninstalling ParchMint does not remove project folders.

## Build from source

Install the pinned tools using the upstream links and platform notes in
[developer setup](../development/setup.md), then:

```sh
git clone https://github.com/pmarathe25/ParchMint.git
cd ParchMint
just bootstrap
just build
just run
```

`just bootstrap` verifies Rust, CMake, and Qt discovery. If Qt is outside the
normal search path, configure `QMAKE` and `CMAKE_PREFIX_PATH` as shown in the
developer guide.
