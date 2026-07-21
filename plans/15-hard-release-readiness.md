# Stage 15: release readiness

Difficulty: hard

Recommended model: strongest available implementation model, high reasoning

Escalate when: a failed platform, accessibility, signing, or compatibility gate requires a product or support-policy decision

Depends on: accepted ADRs 0001–0014 and completion of automated release gates

Master references: [`docs/release/process.md`](../docs/release/process.md), [`docs/release/platforms.md`](../docs/release/platforms.md), and [`docs/release/platform-validation.md`](../docs/release/platform-validation.md)

## Outcome

Produce reproducible, signed ParchMint 1.0 artifacts for every supported
platform with physical accessibility, installer, interoperability, recovery,
and performance evidence. CI or source inspection must not substitute for a
gate that requires real hardware, assistive technology, or consumer software.

## Primary ownership

- Packaging and release workflows
- Platform and accessibility verification
- Export-consumer compatibility evidence
- Release-mode performance evidence
- Licensing, SBOM, signing, notarization, and artifact publication

## Required work

### 1. Build and package release candidates

- Build tagged, locked artifacts for every supported package and architecture.
- Add the required multiresolution Windows `.ico` and inspect branding on every installer and desktop surface.
- Exercise install, upgrade, file association, sleep/resume, crash recovery, and uninstall without project loss.

### 2. Complete physical platform verification

- Run the platform charter on Windows, macOS, Wayland, and X11.
- Validate keyboard-only use, Narrator/VoiceOver/Orca, IME, bidirectional text, scaling, contrast, high contrast, and reduced motion.
- Record OS, architecture, Qt version, hardware, input method, assistive technology, and artifact hash with every result.

### 3. Validate exports and performance

- Open fixtures in Word, LibreOffice, browsers, Qt PDF viewers, EPUBCheck, and representative EPUB readers.
- Run the 10,000-node/10-million-word release budgets and capture timings, memory, file handles, threads, and cancellation latency.
- Investigate failures; never weaken a gate merely to publish.

### 4. Produce and publish release evidence

- Generate SBOM, notices, checksums, signatures, notarization evidence, and artifact hashes from the exact tag.
- Attach CI URLs, physical test records, and measurements to the release-evidence bundle.
- Publish only through the protected production release environment after explicit authorization.

## Verification

- Run every automated release workflow from the tag with locked dependencies.
- Complete the platform charter using the exact candidate artifacts.
- Confirm consumer applications open exported fixtures without undocumented loss.
- Review the evidence bundle against every unchecked release-checklist item.

## Acceptance gate

- Every release gate has a CI URL, artifact hash, measurement, or dated physical test record.
- Every supported platform has complete physical evidence.
- No open P0/P1 data-loss, security, accessibility, packaging, or export-compatibility issue remains.
- Checksums, signatures, SBOM, notices, and publication artifacts all refer to the same protected tag.

## Out of scope

- Changing the GPL-3.0-or-later decision
- Adding an updater, telemetry, or runtime network client
- Expanding supported platforms or package formats during release validation

## Handoff

Record the tag, artifact hashes, CI URLs, physical test matrix, consumer results,
performance measurements, signing/notarization evidence, unresolved external
dependencies, and the exact publication authorization in the plan closure or
release record. Do not create a separate handoff document.
