# Stage 09 handoff: cross-platform hardening and release

Status: release-candidate engineering is implemented and verified on the
available Linux host. **Version 1 is not complete or authorized for release.**
The distribution-license decision, physical Windows/macOS/Linux charters,
consumer export checks, signer/notary evidence, and several stress/resource
measurements remain blocking acceptance items. No artifact was published.

## Working-tree and release identity

- Verification base: `c132b66479b507e13b7f23fc67c9058979117e98`
  plus the Stage 09 changes committed with this handoff (use `git log -1` for
  the non-self-referential final commit ID).
- Application/build version: `1.0.0`; canonical project, Markdown, and recovery
  schemas remain format 1 and are frozen by `docs/release/compatibility.md`.
- Rust 1.97.1 and dynamically linked Qt 6.8.3 were used from
  `.toolchains/qt/6.8.3/gcc_64`.
- Host: Ubuntu 26.04 LTS, Linux 7.0.0-27, x86_64, Intel i7-8550U (4 cores/8
  threads), 7.1 GiB RAM. This older 8 GiB host is not the 2022 reference machine.

## Delivered integration and product behavior

- Added a central Rust command catalog with stable IDs, context-sensitive
  availability, unique-ID tests, and a searchable keyboard command palette.
  Menus and shortcuts dispatch those IDs rather than maintaining a second
  availability implementation.
- Completed literal document replacement, including case choice and replace-all.
  Added project-wide preview with independent per-match selection, a 10,000-row
  bound, preflight Markdown validation, all-document fingerprint conflict checks,
  original-body backups, partial-write rollback, and conflict-protected undo.
- Added persistent system/light/dark settings, bounded recent projects, a
  keyboard map, first-run onboarding, and creation of a real sample project with
  manuscript/research content and two restored panes.
- Added explicit local diagnostics export. Its Rust boundary cannot accept
  project paths or document content; tests cover the report schema, newline
  sanitization, and atomic user-selected output. The privacy policy states the
  local log and diagnostics behavior.
- Destructive binder actions now confirm that trash is canonical/recoverable.
  External attachment opening requires a separate trust/privacy confirmation.
- Refreshed the English translation source catalog from all current QML/C++
  strings (214 active source strings). No guessed non-English translation was
  added.
- Removed the application's direct Qt Network declaration and CXX-Qt network
  module. Qt QML still has a transitive Qt Network library dependency; no
  application network API or request path exists. Deployment excludes TLS,
  network-information, and QML debugging plugins.

## Security and release engineering

- Added a deterministic 10,000-case-by-default parser/package-validator fuzz
  smoke binary and a scheduled workflow for 25,000 cases plus ignored release
  measurements.
- Added local privacy, threat-model, dependency-notice, release checklist,
  compatibility, and support-matrix documents. Release CI runs advisory, source,
  ban, and license policy checks and rejects direct production network APIs.
- Added a locked-metadata CycloneDX 1.5 SBOM and transitive notice generator.
  Tag builds generate per-platform checksums and retain SBOM/notices as CI
  evidence.
- Added dynamic Qt deployment and CPack layouts: WiX MSI + ZIP on Windows, app
  bundle + DMG/TGZ on macOS, and relocatable TGZ on Linux. Linux also has valid
  desktop, AppStream, MIME, scalable icon, AppImage build script, and Flatpak
  manifest. Deployment includes XCB, Wayland, and offscreen platform plugins.
- Added Windows Authenticode and macOS hardened-runtime/notary hook scripts.
  Credentials are required from a protected environment and never stored here.
- Release publication is a separate manual `workflow_dispatch` with `publish`
  explicitly true, a tag ref, all package/evidence jobs passing, write permission
  scoped to that job, and approval from the `production-release` environment.
  Push/tag CI alone cannot create a public release.
- ADR-0012 selects Linux packaging and manual offline updates. ADR-0013 records
  the distribution-license choice as proposed and blocking; engineering packages
  are not permission to distribute.

## Linux artifact and package evidence

The locally built engineering package was not copied into the repository or
published:

| Artifact | SHA-256 | Result |
|---|---|---|
| `ParchMint-1.0.0-Linux.tar.gz` | `f056de5184d2d63050d0d9641d987192050d0c432cd9fb2ba4a2b2ae229770cb` | 39 MiB; fresh extraction and offscreen launch passed |
| `parchmint.cdx.json` | `6581b962d191d9ee67050bfb2f051f4d77dd98cc10f5652219f217237f200472` | Generated from locked Cargo metadata |
| `THIRD_PARTY_NOTICES.generated.md` | `22a227306f51c5e7340ff97901126ac1a5e3f1b62b9d053ab168ee942690bd58` | 10,435 bytes; license expressions/index |

The package contained the executable, relocatable Qt libraries/RPATH, required
QML modules, XCB/Wayland/offscreen plugins, desktop/MIME/AppStream metadata,
privacy statement, notices, and format documentation. Its extracted binary ran
with no toolchain `LD_LIBRARY_PATH`. Network-information, TLS, and QML tooling
plugins were absent. `desktop-file-validate` and `appstreamcli validate --no-net`
passed.

## Automated verification

- `cargo test -p parchmint-app`: 19 passed, 2 manual measurements ignored.
  New command, diagnostics, and replacement tests passed.
- `cargo run --locked -p parchmint-test-support --bin fuzz-smoke -- 1000`:
  passed 1,000 deterministic malformed inputs during the implementation pass.
  Scheduled CI is set to 25,000.
- Release-mode native build with CMake/Qt: passed. `cargo check -p
  parchmint_bridge --offline` passed before the final native link.
- `ctest --test-dir build -C Release --output-on-failure`: 3/3 passed (application
  offscreen smoke, editor adapter, outline model).
- `cmake --build build --target qmllint`: passed with the pre-existing
  unqualified-delegate-access advisories and no errors.
- Fresh CPack extraction: `QT_QPA_PLATFORM=offscreen .../bin/parchmint
  --smoke-test` returned 0 without the development Qt path.
- `cargo test --locked -p parchmint-app -p parchmint-markdown -p
  parchmint-index --release -- --ignored --nocapture`: all 3 measurements passed.
- `git diff --check`, AppStream validation, and desktop-file validation passed.
- `cargo test --workspace --exclude parchmint_bridge --locked`: 52 passed and
  the 3 manual-measurement tests were ignored.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed with the
  pinned Qt environment, including the CXX-Qt bridge.
- `just test` with the pinned Qt environment: passed the Rust workspace,
  desktop build, and all 3 CTest cases.
- `cargo fmt --all --check` and the final `git diff --check`: passed.

No GitHub CI run URL exists for this local worktree. The three-OS workflows are
definitions awaiting a pushed commit/tag; they are not claimed as executed.

## Performance measurements

Measurements are release-mode, local, and noisy/manual unless a test contains
the stated threshold. They are not substitutes for the reference-machine and
physical-GPU charter.

| Measurement | Observed | Budget/result |
|---|---:|---|
| Extracted package offscreen launch/smoke | 0.55 s, 85,788 KiB max RSS | Below 1.5 s; warm/offscreen only |
| 250k-word editor load | 26 ms | Test threshold below 1,000 ms |
| Editor typing, 500 samples | p95 3.697 ms; p99 4.125 ms | Passed 16/50 ms budgets |
| Normal formatting | 48 µs | Passed 50 ms budget |
| Two-pane document load | 18 ms | Passed test threshold |
| Two-pane semantic snapshot | 20 ms | Recorded; snapshot still needs UI-thread architecture review |
| 10k-row tree construction / simulated scroll | 1.289 ms / 1.782 µs | Passed bounded snapshot test |
| 250k-word lifecycle load | 32.436 ms | Recorded |
| UI dirty notification | 0.785 µs | Passed 8 ms UI work budget |
| Recovery journal / canonical save | 19.038 / 9.942 ms | Worker-side timing; durability contract passed |
| 10k-row index rebuild / first results | 372.049 / 56.412 ms | First results passed 300 ms; rebuild is background work |

The 10,000-node/10-million-word fully materialized project-open, binder 60 FPS,
integrated-GPU, slow-disk, repeated open/close handle/thread leak, and idle
two-pane memory measurements were not completed. Storage still eagerly loads
canonical bodies when opening a project; removing that whole-project loading is
a release-blocking performance task, not a waived budget.

## Manual charter and platform status

Only Linux offscreen automation and package extraction were available. Narrator,
VoiceOver, Orca, keyboard-only physical navigation, CJK/IME/dead keys,
bidirectional text, 100/150/200% scaling, high contrast, reduced motion,
integrated graphics, multiple monitors, sleep/resume, abrupt termination,
external-editor races, full disk, permission changes, native install/uninstall,
and long-duration editing/recovery were **not run** in this stage environment.

Word, LibreOffice, EPUBCheck, browsers/readers, and native Qt PDF consumers were
also unavailable. Stage 07 structural validators passed previously, but that is
not consumer evidence. Windows and macOS CI/package builds were not executed
locally. No P0/P1 issue discovered by automated Linux tests is being relabeled as
nonblocking; the missing release evidence remains blocking.

## Licensing, signing, and known issues

- Blocking: ADR-0013 needs a user-approved product license and exact Qt LGPL or
  commercial distribution posture. Qt's complete deployed module license texts
  and replacement/source offer still need to be added to the final notice set.
- Blocking: Windows certificate/icon/MSI validation and macOS identity,
  entitlements, universal/separate-architecture decision, notarization, and DMG
  validation require their release hosts. The Windows packaging directory calls
  out the missing multiresolution `.ico` explicitly.
- Blocking: all physical accessibility/input/lifecycle/consumer charters and the
  stress/resource measurements listed above.
- Blocking: AppImage and Flatpak tools were not installed, so only the Linux TGZ
  was produced and executed.
- Known nonblocking engineering warning: current QML lint reports inherited
  unqualified delegate-role advisories; it reports no type or syntax error.
- Known nonblocking build warning: this host lacks mold/lld/gold, so the native
  release link used GNU `ld.bfd` successfully.

## Exact authorized publication procedure

1. Accept ADR-0013 and finish Qt/application notices, source/replacement offer,
   Windows icon, signing integration, and every blocking checklist/charter item.
2. Push the reviewed commit and signed `v1.0.0` tag. Let `quality-gates`,
   `nightly-hardening` where required, and `release-candidate` finish on all
   platforms. Record their URLs, target OS/hardware, hashes, SBOM, notices,
   signatures, notarization ticket, and consumer results in this handoff.
3. Independently verify checksums and signatures on clean target systems and
   confirm canonical projects survive install, upgrade, `.parchmint/` removal,
   and uninstall.
4. A repository owner starts `release-candidate` manually on the exact tag with
   `publish=true`. The `production-release` environment reviewer compares all
   evidence against `docs/release/checklist.md` and either rejects it or grants
   the explicit authorization.
5. Only the protected `publish` job may run `gh release create`. Store submission
   or any other publication needs its own explicit authorization; there is no
   silent version check or in-app updater.

Recommended next task: refactor project-open storage to load metadata/synopses
without all document bodies, then run the 10-million-word and repeated resource
leak suite before arranging the three physical platform charters.
