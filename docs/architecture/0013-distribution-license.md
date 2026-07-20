# ADR-0013: application distribution license

Status: Proposed — user decision required

## Context

ParchMint dynamically deploys LGPL-available Qt modules and includes Rust
dependencies under permissive terms, but the product's own distribution license
is still `LicenseRef-ParchMint-Undecided`. The repository owner must choose the
application license and confirm the exact Qt distribution obligations before
release artifacts can be published.

## Candidate decision

Select either a compatible open-source application license with complete source,
notices, relinking/replacement instructions where required, and Qt LGPL offers,
or a commercial Qt/application distribution arrangement reviewed by the owner.
Static Qt linkage is not proposed.

## Consequence while proposed

CI may build ephemeral engineering packages for tests. Signing, notarization,
upload to a public release, store submission, or any representation that version
1 is released is prohibited.
