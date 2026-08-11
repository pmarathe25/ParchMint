# `iced_tiny_skia` 0.14.0 transform backport

This directory contains the source and normalized Cargo metadata from the
[`iced_tiny_skia` 0.14.0 package](https://crates.io/crates/iced_tiny_skia/0.14.0).
The original package checksum is
`fe0acf8b75a3bc914aff5f2329fdffc1b36eeaea29dda0e4bd232f1c62e9cc3d`.
Registry bookkeeping and the packaged crate's lockfile are intentionally not
vendored.

ParchMint backports the transform composition used by the official
[`tiny_skia/src/lib.rs` on Iced `master`](https://github.com/iced-rs/iced/blob/master/tiny_skia/src/lib.rs),
as inspected on 2026-08-11. The backport changes only `Renderer::draw`:

- Primitive-group clip bounds are scaled directly. The group transform is
  already represented in the recorded clip bounds.
- Primitive and text group transforms compose the physical scale first:
  `Transformation::scale(scale_factor) * group.transformation()`.

This keeps logical group translations subject to the viewport scale. For
example, a Canvas translated to logical x=100 with a marker at local x=20 is
drawn at physical x=240 at 2x, instead of x=140.

The package declares the MIT license in `Cargo.toml`. The crates.io package did
not include a license file. The official Iced license text is available at
<https://github.com/iced-rs/iced/blob/master/LICENSE>.
