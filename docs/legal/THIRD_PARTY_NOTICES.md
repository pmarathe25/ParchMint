# Third-party notices

> Read when adding dependencies, changing Qt linkage/modules, packaging notices,
> or generating release evidence.

ParchMint itself is licensed under GPL-3.0-or-later (see `LICENSE` and
ADR-0013). This checked-in notice is the reviewed dependency policy summary.
Each distributed release must also include the machine-generated transitive
notice and CycloneDX SBOM produced from its exact `Cargo.lock` and Qt
deployment.

- Qt 6.8.3 is dynamically deployed under the license selected by the distributor;
  open-source use must satisfy the applicable LGPL/GPL module terms and include
  Qt's own notices and source/replacement information.
- CXX-Qt, cxx, serde, tempfile, tracing, uuid, toml, pulldown-cmark, rusqlite and
  other locked Rust dependencies use the permissive licenses admitted by
  `deny.toml`, primarily MIT, Apache-2.0, BSD, ISC, Unicode-3.0, and Zlib.
- Bundled SQLite is dedicated to the public domain.

`cargo deny check licenses` is the release policy gate. Unknown, unlicensed, or
unapproved terms block packaging; they are never silently omitted from notices.
