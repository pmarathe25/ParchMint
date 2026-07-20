# ParchMint

ParchMint is a native, local-first long-form writing application under active
development. The Stage 01 scaffold combines Rust application services with Qt 6
Quick/QML through CXX-Qt; it does not use a browser or WebView.

The product and architecture contract is in [`PLAN.md`](PLAN.md). Start with
[`docs/development/bootstrap.md`](docs/development/bootstrap.md) to build the
foundation application.

Developer navigation, commands, the regression matrix, and platform charter
are in [`docs/development/`](docs/development/). The user guide is in
[`docs/user-guide/`](docs/user-guide/), and the exporter support contract is in
[`docs/export/support-matrix.md`](docs/export/support-matrix.md).

The final distribution license is intentionally not selected. Do not publish
artifacts or statically link Qt until the licensing ADR required by the product
contract is accepted.
