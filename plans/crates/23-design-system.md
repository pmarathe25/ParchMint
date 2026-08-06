# Design system

## Goal

Generate deterministic semantic UI tokens and vectors from the maintained design source.

## Depends on

- [01 Bootstrap CI and supply chain](../integration/01-bootstrap-ci-and-supply-chain.md)

## Owning crate(s)

[`parchmint-design-system`](../../docs/architecture/crates/parchmint-design-system.md)

## Requirements and UI design

- [Foundations](../../docs/ui-design/foundations.md)
- [UI design source and stable names](../../docs/ui-design/README.md)
- [Screen catalog](../../docs/ui-design/screen-catalog.md)
- [Appearance](../../docs/product/appearance.md)

## Work

- Parse the token and SVG source, validate aliases and checksums, require Light/Dark role parity, normalize values, and generate framework-neutral Rust data.

## Stage-specific tests and validation

Fail on alias cycles, missing roles/icons, changed vector checksums, or nondeterministic generated output; verify Source Sans 3 normalization and Light/Dark parity.
