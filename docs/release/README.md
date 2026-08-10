# Native release process

ParchMint releases one candidate set for Windows, macOS, and Linux. A candidate
set is releasable only after the verifier finds every package and every
structured evidence file, validates their SHA-256 digests, and confirms the
evidence belongs to the same version, source revision, platform, and package.

No release candidate exists in this repository. `packaging/release-inputs.toml`
records the inputs that are still missing without assigning unsupported minimum
platform versions or claiming native validation occurred.

## Files and ownership

- `packaging/release-inputs.toml` records package definitions and unresolved
  release inputs. Normal CI validates this file on all three runner platforms.
- `packaging/<platform>/` contains templates for MSIX, DMG application-bundle,
  and Debian package metadata. A native packaging job replaces template values
  only after their release inputs are approved.
- `packaging/schemas/release-inputs.schema.json` and
  `release-candidates.schema.json` define readiness state and the candidate
  manifest exchanged by packaging and verification jobs.
- `packaging/schemas/signing-inputs.schema.json` defines references to signing
  and notarization credentials. It accepts environment-variable names, not
  credential values.
- The remaining schemas define minimum-version runs, dependency notices,
  provenance, release-gate results, and signature, notarization, clean-install,
  launch, upgrade, uninstall, and native UI observations. The generated
  `release/` records use TOML (for example `provenance.toml` and
  `release-gates.toml`) even though their data-model schemas are JSON Schema.
- `release/` is a generated candidate workspace. Do not add placeholder
  packages or passing evidence to satisfy the verifier.

The schemas describe the data model after TOML decoding. Release jobs write
TOML so `parchmint-ci` can apply the same strict unknown-key and value checks
without adding a second runtime parser.

## Candidate workflow

1. Run native CI and runtime validation on proposed minimum platform versions.
   Change each `minimum-version-status` to `frozen` only with a repository-local
   evidence path from those runs.
2. Approve signing and notarization policy. Create input documents that match
   the signing-input schema, bind the real package SHA-256, and reference CI
   secret names. Keep secret values in the CI secret store.
3. Build the exact source revision with `Cargo.lock` and the pinned Rust
   toolchain. The native job renders its package template and creates the real
   package.
4. Run signature verification on the signed package. On macOS, run
   notarization, staple the result, and verify the stapled package. Record the
   verification tool and the notarization service ticket in the corresponding
   passed evidence records.
5. On a clean native runner, install the package, launch it, upgrade from the
   supported predecessor, and uninstall it. Record each passed observation as a
   separate evidence document. Record menus, dialogs, and clipboard checks as
   `kind = "native-ui"` with all three check names.
6. Generate the reviewed dependency notices and release SBOM. Preserve the
   existing SBOM baseline policy: changes to `Cargo.lock` or bundled artifacts
   require a reviewed baseline update.
7. Create provenance that binds the source revision, `Cargo.lock`, notices,
   SBOM, package paths, and SHA-256 digests to the native build runs.
8. Create `packaging/release-candidates.toml` from the real files and run:

   ```text
   cargo parchmint-ci release verify
   ```

The release-tag CI gate runs the same command only for `v*-rc*` tags. It also
requires the tag to equal `v` plus the manifest release version, and fails when
an input, package, digest, signature, lifecycle observation, native UI result,
or release-gate result is missing or deferred. Repository paths must be
portable relative paths; absolute, dot-segment, Windows-drive, backslash, and
symbolic-link evidence paths are rejected.

## Supply-chain policy

Release verification reapplies the bootstrap controls before reading candidate
evidence. It checks the lockfile-derived SBOM, bundled-artifact hashes, and
exception records. Every advisory, license, provenance, source, bundled
artifact, or SBOM exception retains a package, owner, reason, and expiry.
Expired exceptions fail verification and require a new review; release work
must not bypass them through `deny.toml`.

## Current blockers

- Native runtime evidence does not support minimum Windows, macOS, or Linux
  versions.
- Native package icons and Windows logo assets have not been approved or
  exported.
- The desktop crate assembles the production graph, but its production runner
  does not enter a real native event loop. It cannot yet produce honest launch
  or native UI evidence.
- No signing identities, notarization profile, CI credential references, or
  Linux signing policy are approved.
- No clean install, launch, upgrade, uninstall, signature, notarization,
  dependency-notice, provenance, or complete release-gate evidence exists.
