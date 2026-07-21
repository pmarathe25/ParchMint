# Supported platforms

> Read when changing OS targets, architecture, package formats, or platform assumptions.

| Platform | Architecture | Minimum | Packages |
|---|---|---|---|
| Windows | x86_64 | Windows 10 22H2 | WiX MSI, portable ZIP |
| macOS | arm64 primary; x86_64 separate | macOS 13 | Hardened `.app`, DMG, TGZ |
| Linux | x86_64 | Ubuntu 24.04-equivalent runtime | AppImage; Flatpak/TGZ alternatives |

Qt is dynamically deployed. Linux supports Wayland and X11 fallback. Each
platform must pass keyboard, screen-reader, IME/dead-key, bidirectional-text,
100/150/200% scaling, high-contrast, reduced-motion, multi-monitor, install,
upgrade, and recovery validation.

Projects are ordinary files and remain portable across supported platforms.
Changing a minimum OS, architecture, or package promise requires product-owner
approval, packaging changes, validation updates, and usually an ADR.
