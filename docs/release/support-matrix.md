# Version 1 support matrix

Status: release-candidate target; rows require physical evidence before release.

| Platform | Architecture | Minimum target | Package | Status |
|---|---|---|---|---|
| Windows | x86_64 | Windows 10 22H2 | WiX MSI, portable ZIP | CI definition present; physical install/signing/Narrator pending |
| macOS | arm64 primary; x86_64 separate | macOS 13 | hardened `.app` in DMG | CI definition present; signing/notarization/VoiceOver pending |
| Linux | x86_64 | Ubuntu 24.04 or equivalent | AppImage primary; Flatpak manifest/TGZ fallback | Linux build/tests pass; AppImage/Orca distro matrix pending |

Qt is dynamically deployed. Wayland and X11 fallback are targets. Integrated
graphics, 100/150/200% scaling, high contrast, reduced motion, IME/dead keys,
bidirectional text, and multiple monitors remain part of the platform charter.
Projects are ordinary files and remain supported without ParchMint on every OS.
