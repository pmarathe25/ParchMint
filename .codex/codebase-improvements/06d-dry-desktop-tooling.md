# Stage 6 DRY audit — desktop/tooling (group D)

Scope reviewed: `crates/parchmint-desktop/src/production.rs`,
`crates/parchmint-core-cli/src/lib.rs`, `tools/parchmint-ci/src/main.rs`,
packaging/release configuration, and root workspace configuration. Source
inspection only; no builds, tests, metadata, or heavy scans were run.

## Findings

### D1 — Core CLI repeats project-root acquisition and lease handling

Confirmed evidence: `crates/parchmint-core-cli/src/lib.rs` has the same
three-step sequence in `save` (around lines 474–480), `checkpoint`
(around 603–609), `restore` (around 642–648), and `history` (around
669–675): construct `NativeProjectFileSystem`, acquire an
`UntrustedProjectPath`, and map the `FsError` through `filesystem_outcome`.
`pending_recovery` (around 923–929) repeats the sequence as well, with a
manually dropped lease. The functions then independently construct the
history/save/recovery services from the returned root.

This is production CLI behavior, not test scaffolding. A future change to
project-root acquisition, path authorization, or lease lifetime can update
one command and silently leave the others with different safety/lifecycle
semantics. It also makes it easy to accidentally omit the lease drop or use
the path rather than the authorized root when wiring a service.

Smallest solution: add one private local helper (for example
`acquire_project_root(&Path) -> Result<(NativeProjectFileSystem,
ProjectRootCapability, ProjectLockLease), Outcome>`, using the existing error
mapping) and route these call sites through it. Keep `open_project` separate:
it performs repository validation and pending-restore reconciliation, so the
helper must not merge or reorder that lifecycle. The helper should return the
lease explicitly so existing command lifetimes remain visible and unchanged.

Validation: compare each migrated command's error mapping and lease scope
with its current behavior; exercise locked, unsafe, missing, and valid project
paths for save/checkpoint/restore/history/index/recovery flows. No API or CLI
contract changes are required.

### D2 — Release verification manually enumerates homogeneous evidence checks

Confirmed evidence: `tools/parchmint-ci/src/main.rs::verify_release` repeats
the same `verify_release_file_hash(path, digest, label)` call for the four
manifest artifacts (`dependency_notices`, `sbom`, `provenance`, and
`release_gate_evidence`) around lines 450–471. The candidate section then
repeats `verify_release_evidence(path, digest, kind, &manifest, candidate)`
for install, launch, upgrade, uninstall, and conditionally signature,
notarization, and native UI evidence around lines 478–553. The helper
functions already centralize the actual file/hash and identity checks; only
the hand-maintained call tables are duplicated structure.

This creates omission/divergence risk when a new evidence kind or manifest
artifact is added: a field can parse and be included in the manifest while a
verification call is forgotten, or receive an inconsistent label. It is
especially consequential because this tool is the release gate and its output
is the release evidence contract.

Smallest solution: build fixed local arrays of `(path, digest, label)` for the
four manifest artifacts and `(path, digest, kind)` for the mandatory candidate
evidence, iterate them, and retain the existing conditional branches for
signature/notarization/native UI (including their policy/deferred behavior).
Do not make the parser/schema generic or move release policy into a broad
utility crate; the tuples should remain adjacent to `verify_release` so the
release contract stays auditable.

Validation: mutate each tuple source in a fixture (wrong digest, missing path,
wrong evidence kind, and omitted mandatory evidence) and confirm the same
failure labels and rejection behavior. Confirm deferred native UI and
not-applicable signing still follow their current branches.

## Non-findings / boundaries

No additional high-confidence production finding is reported. Desktop
`assemble_with_controls` is intentionally the composition boundary, and the
parallel setup visible in production tests was excluded as test-only. The
platform/package strings duplicated between packaging JSON schemas and CI
parsers are a release-contract concern, but extracting them would require a
schema-generation or runtime-schema design decision rather than a small local
DRY change; it is therefore left for a dedicated contract/schema audit.
