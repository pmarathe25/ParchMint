# Contracts

## Goal

Define versioned JSON contracts and deterministic generated Rust bindings.

## Depends on

- [01 Bootstrap CI and supply chain](../integration/01-bootstrap-ci-and-supply-chain.md)

## Owning crate(s)

[`parchmint-contracts`](../../docs/architecture/crates/parchmint-contracts.md)

## Requirements and UI design

- [Canonical project data](../../docs/product/canonical-project-data.md)
- [Privacy and security](../../docs/product/privacy-and-security.md)

## Work

- Add schemas for annotation sidecars, recovery records, and machine-readable CLI output with pinned generation.
- Version incompatible changes and keep schema-specific fixtures with the contract source.

## Stage-specific tests and validation

Validate schemas, decode and re-encode every fixture through generated types, and fail regeneration when generated files differ.
