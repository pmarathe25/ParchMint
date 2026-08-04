# S120 — Release Hardening

## Goal

Produce installable, secured, supportable candidates on all target platforms.

## Tasks

- Windows installer/upgrade/uninstall/WebView2/project-lock/single-instance/native input/accessibility.
- macOS app/DMG/signing/notarization/WKWebView/native menus/VoiceOver.
- Linux `.deb`/supported WebKitGTK matrix/Wayland/X11/Orca. AppImage remains deferred.
- Clean-machine install/launch/uninstall.
- Cross-platform project interchange.
- Migration/upgrade testing.
- Exact locks, advisories, provenance, licenses/notices, SBOM, package hashes.
- Final Light/Dark resources and offline spellcheck language/dictionary packages.

## External inputs

Signing/notarization credentials or paid infrastructure may require an explicit external-input stop. Preserve completed evidence and state exactly what is needed.

An S120 run that changes installers, security configuration, bundled resources, upgrade behavior, or any other shipped package behavior requires the paired independent test challenge. A pure evidence rerun may be exempt only when dispatch records that no shipped artifact or behavior changes.
