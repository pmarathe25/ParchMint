# Stage 09: cross-platform hardening and release

Difficulty: very hard  
Recommended model: GPT-5.6 Sol, high or max reasoning  
Cost-conscious fallback: GPT-5.6 Terra, max reasoning, with Sol reviews for unresolved platform/data issues  
Depends on: stages 1–8  
Master references: `PLAN.md` “Cross-stage quality gates,” “Open release decision,” and “Version 1 definition of done”

## Outcome

Integrate and harden ParchMint into a performant, accessible, secure, installable version 1 release on Windows, macOS, and Linux. This stage owns cross-feature failures and release-environment work; it must not weaken data-integrity or format guarantees to make a release pass.

## Primary ownership

- Cross-cutting production fixes with regression tests
- Command palette, find/replace completion, settings, onboarding, and diagnostics
- Accessibility and localization readiness
- Performance profiling and regression budgets
- `packaging/*`, release CI, signing/notarization hooks, manifests, and notices
- Release checklist, privacy statement, threat model, and final support matrix

## Required work

### 1. Cross-feature integration audit

- Run every end-to-end flow from `PLAN.md` across project create/open, outlining, editing, styles, recovery, research, splits, search, compile, and export.
- Resolve stale revision, focus routing, undo-boundary, workspace restoration, and shutdown races.
- Test sleep/resume, abrupt termination, multiple projects/windows if supported, external edits, removed assets, full disk, and permission changes.
- Close every blocker from the stage 8 traceability matrix or explicitly remove the affected feature from version 1 with user approval and updated scope.

### 2. Product completion

- Implement command palette over the central command registry.
- Complete document find/replace and add project-wide replacement only with preview, per-change selection, undo/recovery, and conflict protection.
- Complete recent projects, settings, theme controls, keyboard map, onboarding/sample-project flow, diagnostics export, and actionable error UX.
- Ensure destructive operations use recoverable trash/backup where specified and clearly state permanence.

### 3. Accessibility and UX hardening

- Audit names, roles, states, focus order, focus visibility, keyboard reachability, high contrast, reduced motion, and 200% scaling.
- Test Narrator on Windows, VoiceOver on macOS, and Orca on Linux using the stage 8 charters.
- Fix dynamic type/layout clipping and theme contrast failures.
- Verify bidirectional text and IME in the integrated application, not only the editor harness.
- Confirm every pointer-only action has a discoverable keyboard/menu equivalent.

### 4. Performance and resource hardening

- Profile cold launch, stress-project open, editor input, tree/outline scrolling, autosave, indexing, search, two-pane memory, compile, and export.
- Remove whole-project/body loading, unnecessary QML delegate creation, synchronous UI-thread work, and unbounded task queues.
- Add stable automated regression thresholds and record noisy/manual measurements separately.
- Test on integrated graphics, high-DPI/multiple monitors, slow disks where reproducible, and the documented reference machine.
- Detect memory/file-handle/thread leaks across repeated project open/close cycles.

### 5. Security and privacy review

- Re-audit path traversal, symlink escape, hostile manifests/front matter, zip/package export, attachment previews, external-open actions, and temporary files.
- Fuzz smoke-test all parsers with current corpora and triage crashes/hangs.
- Verify the application makes no network request in normal operation.
- Produce a clear privacy statement and diagnostics policy.
- Generate third-party dependency/license notices and resolve disallowed/unknown licenses.

### 6. Packaging and release engineering

- Produce Windows installer/package with uninstall, file association if approved, icons, version metadata, and signing hooks.
- Produce macOS universal or documented architecture builds, app bundle, entitlements, hardened runtime, notarization hooks, icons, and file association if approved.
- Produce Linux AppImage and/or Flatpak according to an ADR, desktop entry, MIME registration, icons, and documented distro/runtime support.
- Bundle or dynamically deploy Qt correctly and record license compliance. Do not statically link until the licensing ADR permits it.
- Make release CI build from tags, run tests, generate checksums/SBOM/notices, and publish artifacts only through an explicitly authorized release step.
- Define an update strategy; an in-app updater may be deferred, but version checks must not be silently added to the offline application.

### 7. Release validation

- Test fresh install, upgrade from every existing prerelease schema, create/write/recover/compile, and uninstall on clean target systems.
- Validate saved projects outside ParchMint after installation/uninstallation.
- Run long-duration editing/autosave and repeated crash-recovery exercises.
- Freeze format version 1 and public compatibility promises before the release candidate.

## Acceptance gate

- All master-plan definition-of-done flows pass on Windows, macOS, and Linux release builds.
- No open P0/P1 correctness, data-loss, security, accessibility, or packaging defects remain.
- All performance budgets pass or have a user-approved documented exception supported by measurements.
- Screen-reader, keyboard-only, IME, high-DPI, and reduced-motion charters pass.
- Installers are reproducible enough for the release policy, contain required notices, and install/uninstall cleanly.
- A project remains fully readable and complete without `.parchmint/`, SQLite, or the installed application.
- Release artifacts, checksums, SBOM, notices, privacy statement, format docs, and support matrix are complete.
- No external release or publication occurs without explicit user authorization.

## Handoff

Create `docs/handoffs/09-cross-platform-hardening-and-release.md` as the release evidence report. Include artifact hashes, CI runs, platform versions/hardware, manual charter results, benchmark tables, known nonblocking issues, licensing decision, signing/notarization status, and the exact authorized publication procedure. Mark version 1 complete only when every acceptance item is evidenced.
