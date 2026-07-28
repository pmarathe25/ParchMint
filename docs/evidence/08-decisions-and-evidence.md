# ParchMint Final Decisions and Evidence Record

**Status:** Accepted v1 decision record  
**Version:** 1.0  
**Date:** 2026-07-28

## 1. Purpose

This document records which exploration conclusions are accepted as evidence, which decisions are product-owner choices made in light of incomplete evidence, and which alternatives are closed for v1.

Historical reports under `evidence/reports/` remain unmodified. This document and `02-final-architecture.md` govern v1 implementation.

## 2. Final decisions

| Area | v1 decision | Status |
|---|---|---|
| Desktop shell | Tauri 2.11.5 | Selected by product decision |
| Application UI | TypeScript + React | Selected |
| Rich editor | Exact-locked ProseMirror, behind ParchMint adapter | Selected by product decision |
| Canonical documents | Restricted deterministic HTML5 | Selected |
| Project metadata | TOML + CSS + JSON sidecars | Selected |
| History | `git2 =0.21.0`, vendored libgit2 | Evidence-backed selection |
| Search | `rusqlite =0.40.1`, bundled SQLite FTS5 | Evidence-backed selection |
| Initial export | Self-contained HTML5 | Selected |
| Platforms | Windows, macOS, Linux from v1 | Selected |
| Large documents | Same behavior for all supported documents; no special mode | Product-owner decision |
| Native Rust editor | Closed for v1 | Rejected |
| Third frontend comparison | Closed for v1 | Rejected |

## 3. Frontend evidence disposition

### V01 native GPUI composition

The GPUI + `text-document` + `text-typeset` candidate failed the exploration’s hard gates:

- No single geometry authority for painting, hit testing, caret, selection, IME, and accessibility.
- No real IME integration.
- Accessibility not demonstrated.
- First editable pipeline at least 280.356 ms under its measured setup.
- Windows/macOS interactive evidence absent.
- System-font configuration reached approximately 903,732 KiB peak RSS.

The native composition would require a substantial custom editor/view/accessibility bridge. It is rejected for v1 rather than extended.

### V02 and V02-R Tauri/ProseMirror control

Accepted positive evidence:

- Exact locks remained unchanged.
- Semantic tests passed.
- Canonical before/after fixtures remained byte-identical.
- Native packages were produced for Windows, macOS, and Linux.
- A corrected native Linux WebKitGTK development viewport loaded the accepted approximately 248,079-word fixture with 1,923 mounted blocks in each of two views.
- View-to-view propagation in a diagnostic trace was fast relative to input timing.

Unresolved/negative evidence:

- Packaged Linux fixture loader was defective under `tauri://localhost`.
- AppImage failed on the host because of GLib/GVFS/EGL issues.
- Corrected development two-view first editable viewport measured 637–640 ms, above the exploration’s 250 ms gate.
- Diagnostic typing samples were not isolated/acceptable and cannot be treated as a production measurement.
- Real CJK IME, usable screen-reader editing, high-DPI behavior, long-run memory stability, and native Windows/macOS launch/runtime remained unknown.

The exploration therefore returned `frontend: none`. The product owner subsequently chose to proceed with Tauri/ProseMirror, end framework exploration, keep the 250,000-word support goal, and reject any v1 user-visible large-document mode.

This is a conscious risk acceptance. Implementation and release validation must produce the missing native evidence.

## 4. History evidence

V03 passed and is accepted as the basis for selecting `git2`.

Validated composition:

```toml
git2 = { version = "=0.21.0", default-features = false, features = ["vendored-libgit2"] }
```

Resolved native layer:

- `libgit2-sys 0.18.7+1.9.6`
- libgit2 1.9.6
- Vendored true
- HTTPS/SSH disabled
- Static zlib release guard

Key results:

- 250,000 checkpoints: p50/p95/p99 `1.654/4.893/5.700 ms`.
- 1,000,000 checkpoints: p50/p95/p99 `0.698/0.825/1.384 ms`.
- One-million pack: approximately 98,466,494 bytes.
- One-million peak RSS: approximately 1,349,592 KiB.
- One-million pack creation: approximately 214.753 seconds.
- Native 10,000-checkpoint functional smoke passed on Linux, Windows, and macOS.
- Same repository continued Linux → Windows → macOS with clean worktrees and Unicode/long-path invariants.
- Kill tests during object/tree/ref/pack/maintenance preserved the last completed checkpoint.

Accepted architecture consequences:

- One linear app-managed `main`.
- Bounded unsorted history paging.
- Additive restore.
- Exclusive-owner cleanup of stale `main.lock` after validation.
- ParchMint-owned pack/verify/cleanup policy.
- Background maintenance only.
- No `gix` comparison unless a future concrete failure appears.

## 5. Search evidence

V04 passed and is accepted as the basis for selecting bundled SQLite FTS5.

Validated composition:

```toml
rusqlite = { version = "=0.40.1", default-features = false, features = ["bundled"] }
```

Key full-scale result:

- Exactly 20,000,000 words.
- 550 documents.
- 167,074 blocks.
- Initial index: approximately 9.634 seconds.
- Rebuild: approximately 9.360 seconds.
- Database size: 281,280,512 bytes.
- Worst first-result p99: 11.979 ms.
- 250,000-word document replacement: 141.120 ms.
- Integrity, deletion/rebuild, streaming, cancellation, query escaping, stale-result revalidation, and concurrency passed.
- Native Linux/Windows/macOS parity passed on the shared smoke corpus.

Accepted architecture consequences:

- Dedicated SQLite worker/connection.
- Startup FTS5 assertion.
- External-content FTS table with stable block/revision IDs.
- Field-aware body/title/Synopsis/metadata index.
- Safe quoted MATCH generation and allow-listed fields.
- Case-sensitive/whole-word post-filtering.
- Streaming/cancellation/revalidation.
- Disposable deterministic rebuild.
- No Tantivy comparison unless a future concrete failure appears.

## 6. Exact reference locks

The final V02/V02-R reports preserved:

| Lock | SHA-256 |
|---|---|
| V02 `Cargo.lock` | `c459d31ef0717bde10fd366a4151d4f781984284dc33d135c41d9eadea51f2c9` |
| V02 `package-lock.json` | `43adb95f615d22d073973b54d5e6cfec5ac96e350edd454c0cd74158f9a71a83` |
| Validation `Cargo.lock` | `84f440580af8d156e19932f8006aa0cc49f11f7c2985283584fe952915499c77` |

Reference copies are under `evidence/reference-locks/`. They are provenance inputs, not the final application lockfiles. The implementation adds React and other application dependencies, then commits new exact locks while preserving the validated ProseMirror package versions until an approved upgrade ADR.

## 7. Closed alternatives for v1

- GPUI + `text-document` + `text-typeset`.
- Freya.
- Floem.
- GTK.
- egui custom editor.
- `taino-edit-core` production editor.
- Rust `prosemirror` as the production view.
- `gpui-component` baseline.
- `gix` history comparison.
- Tantivy search comparison.
- ProseMirror virtualization exploration before implementation evidence.
- A third frontend comparison.

These may be reconsidered only through a future ADR with a concrete failure or materially changed upstream evidence.

## 8. Residual v1 risks

1. Tauri webviews differ across WebView2, WKWebView, and WebKitGTK.
2. Full ProseMirror DOM behavior at 250,000 words and two views has incomplete release-mode evidence.
3. Native IME, accessibility, high-DPI geometry, and long-run memory must still be proven on all platforms.
4. Linux packaging must avoid the failed V02-R fixture-loading path and must not assume AppImage viability.
5. Shared ProseMirror document history with independent selections across two views requires careful custom session control.
6. The ProseMirror package provenance should be monitored because the former aggregate GitHub repository was archived while individual packages continue separately.
7. Rust advisory/license automation must be established in the real repository.

Mitigation is early implementation validation, strict adapter boundaries, exact locks, cross-platform CI/runtime testing, and stop conditions in the implementation plan.

## 9. Decision ownership

Evidence-backed selections:

- `git2` history.
- SQLite FTS5 search.
- Rejection of the explored native GPUI composition.

Product-owner choices beyond evidence:

- Select Tauri/ProseMirror despite strict exploration failure/incompleteness.
- Treat all supported documents identically.
- Continue to target approximately 250,000 words per document.
- End architecture exploration and proceed to design/implementation.

These distinctions must remain visible in future reviews.
