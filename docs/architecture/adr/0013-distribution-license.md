# ADR-0013: application distribution license

Status: Accepted

## Context

ParchMint dynamically deploys LGPL-available Qt modules and includes Rust
dependencies under permissive terms. The application license itself was left
undecided during implementation (`LicenseRef-ParchMint-Undecided`), blocking
signing, notarization, and publication of release artifacts.

## Decision

ParchMint is licensed under **GPL-3.0-or-later**. The full license text lives in
[`LICENSE`](../../../LICENSE) at the repository root; crate manifests, the Linux
AppStream metainfo, and package metadata use the SPDX identifier
`GPL-3.0-or-later`.

Qt continues to be linked dynamically (ADR-0001). Distribution must satisfy the
applicable Qt LGPL obligations: retain Qt's own notices and license texts,
provide Qt source/relinking information, and never statically link Qt under this
configuration. Third-party Rust dependencies remain constrained to the
permissive licenses admitted by `deny.toml`; the workspace's own GPL license is
exempt from that dependency allow-list via `[licenses.private]`.

## Consequences

- Source availability is mandatory for anything published: releases ship with
  the corresponding source (this repository) and complete notices.
- The release workflow's publication gate remains: signing, notarization, and
  store submission still require the protected `production-release`
  environment, but the license blocker itself is resolved.
- Derivative works distributed by others must also be GPL-3.0-or-later.
