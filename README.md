# ParchMint

ParchMint is a native, local-first long-form writing application. The version 1
release candidate combines Rust application services with Qt 6 Quick/QML through
CXX-Qt; it does not use a browser, WebView, or network client.

The product and architecture contract is in [`PLAN.md`](PLAN.md). Start with
[`docs/development/bootstrap.md`](docs/development/bootstrap.md) to build the
foundation application.

Developer navigation, commands, the regression matrix, and platform charter
are in [`docs/development/`](docs/development/). The user guide is in
[`docs/user-guide/`](docs/user-guide/), and the exporter support contract is in
[`docs/export/support-matrix.md`](docs/export/support-matrix.md).

Engineering packages and release automation are available under `packaging/`
and `.github/workflows/release.yml`. The final distribution license is not yet
selected: do not sign, notarize, publish, or statically link Qt until ADR-0013 is
accepted and the protected release environment explicitly authorizes publishing.
