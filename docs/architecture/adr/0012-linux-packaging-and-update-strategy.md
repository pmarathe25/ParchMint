# ADR-0012: Linux packaging and offline update strategy

Status: Accepted

## Decision

Build a dynamically linked Qt application and use AppImage as the primary Linux
release candidate, with a CPack `.tar.gz` engineering package and a Flatpak
manifest as reproducible fallbacks. AppImage tooling must be supplied by CI; no
packaging script downloads or executes an unpinned tool. The supported baseline
is 64-bit Ubuntu 24.04 or a contemporary distribution with equivalent kernel,
graphics, and desktop facilities. Wayland and X11 fallback are supported.

Windows uses WiX plus a portable ZIP. macOS uses a hardened app bundle in a DMG
plus a `.tar.gz`; Apple silicon is primary and x86_64 must be built separately
unless a universal Qt SDK is explicitly supplied. Every build dynamically
deploys Qt with Qt's CMake deployment API.

Version 1 has no updater and makes no version-check request. Updates are manual:
the user downloads an authorized signed package and installs it over the prior
application. Canonical project schemas remain independently versioned.

## Publication boundary

These choices authorize engineering packages. ADR-0013 resolves the application
license as GPL-3.0-or-later; publishing still requires the protected release
environment and the evidence in the release checklist.
