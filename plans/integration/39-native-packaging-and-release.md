# Native packaging and release

## Goal

Produce verified Windows, macOS, and Linux release candidates.

## Depends on

- [38 Complete application](38-complete-application.md)

## Owning paths

Packaging definitions, signing/notarization inputs, installer tests, release automation, notices, SBOMs, and release documentation.

## Requirements and UI design

- [Platform scope](../../docs/product/platform-scope.md)
- [Release gates](../../docs/product/release-gates.md)
- [Privacy and security](../../docs/product/privacy-and-security.md)
- [Foundations](../../docs/ui-design/foundations.md)
- [Platform conventions](../../docs/ui-design/platform-conventions.md)
- [Screen catalog](../../docs/ui-design/screen-catalog.md)

## Work

- Build installable candidates, dependency notices, and release SBOMs. Reapply the bootstrap supply-chain policy: locked dependencies, advisory/license checks, provenance/source checks, bundled-artifact hashes, SBOM diff, and exception owner/reason/expiry review.
- Freeze minimum Windows, macOS, and Linux-distribution versions from native CI and runtime validation before public beta.

## Stage-specific tests and validation

On each supported platform, validate signed/package integrity where applicable, clean install, launch, upgrade, uninstall, native menus/dialogs/clipboard, dependency notices, SBOM/provenance/hash checks, and final release-gate results.
