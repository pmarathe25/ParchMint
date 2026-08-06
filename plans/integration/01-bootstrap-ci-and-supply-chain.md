# Bootstrap CI and supply chain

## Goal

Create the workspace, reproducible toolchain, and cross-platform CI baseline.

## Depends on

None.

## Owning paths

Workspace manifests, lockfile, toolchain configuration, CI workflows, and dependency-policy configuration.

## Requirements and UI design

- [Platform scope](../../docs/product/platform-scope.md)
- [Privacy and security](../../docs/product/privacy-and-security.md)

## Work

- Add the Rust workspace, committed lockfile, pinned generators, and initial crate layout.
- Run CI on pull requests and merged changes that affect dependencies, bundled assets, packaging, or CI; rerun it for every release candidate.
- Require locked dependencies, advisory and GPL-compatible license checks, provenance/source checks, bundled-artifact hashes, SBOM generation and diff. Every supply-chain exception must record an owner, reason, and expiry in the repository's exception record.

## Stage-specific tests and validation

Windows, macOS, and Linux run workspace metadata, formatting, linting, locked dependency resolution, advisory/license/provenance checks, hash verification, and SBOM diff.
