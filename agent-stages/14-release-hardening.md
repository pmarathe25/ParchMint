# S120 — Cross-Platform Release Hardening

## Goal

Produce installable, reproducible release candidates and complete platform-specific operational work.

## Tasks

Follow Phase 8 and the acceptance plan:

- Windows installer, WebView2 behavior, upgrade/uninstall, file locks, high DPI, shortcuts, clipboard/drag/drop, and screen reader.
- macOS app/dmg, signing/notarization workflow, WKWebView, menus/dialogs, VoiceOver, scaling, path normalization.
- Linux deb/runtime dependency matrix, X11/Wayland, WebKitGTK variants, clipboard/IME/drag/drop, Orca/AT-SPI; do not claim AppImage support unless separately proven.
- Shared migration, clean-machine install/launch/uninstall, SBOM/notices/advisory scans, package provenance, and project interchange.

## External inputs

When signing/notarization credentials are unavailable, finish all unsigned/reproducible work and produce an exact credential/input request. Do not fabricate signed evidence.

## Pass criteria

Release candidates install, launch, upgrade, migrate, and interchange correctly on all supported platforms; package provenance and notices are complete.
