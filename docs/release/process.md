# Release process

> Read when building, validating, signing, or publishing a release artifact.

A release is one protected tag, one locked source/dependency state, and one
evidence bundle tying every artifact to that tag.

## Gates

1. **Source:** clean tag; accepted license/format decisions; no P0/P1 correctness, security, accessibility, or packaging defect.
2. **Automated:** format, lint, tests, fuzz smoke, release performance budgets, dependency/advisory policy, SBOM, notices, and package inspection.
3. **Platforms:** install, upgrade, association, launch, IME, accessibility, scaling, sleep/resume, recovery, and uninstall on each supported OS.
4. **Exports:** structural validation plus real consumer opening for every advertised format.
5. **Supply chain:** checksums, signatures, notarization, source offer, notices, SBOM, and artifact hashes.
6. **Publication:** explicit authorization through the protected production environment.

## Evidence bundle

Record the tag and commit, artifact hashes, CI URLs, toolchain versions,
platform/hardware details, validation results, performance measurements,
signatures, notarization records, SBOM, and notices. A synthetic or inferred
result cannot satisfy a physical gate.

Use the [platform targets](platforms.md), [validation charter](platform-validation.md),
[performance budgets](../development/performance.md), and
[export fidelity](../reference/export-fidelity.md).
