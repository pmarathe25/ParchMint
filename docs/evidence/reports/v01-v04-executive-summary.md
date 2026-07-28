# V01–V04 executive summary

**Decision:** `production_frontend: none`.

V01 fails the single-geometry, real-IME, accessibility, first-viewport, and
all-platform interactive gates. V02 passes shared semantic/web-build tests and
native package creation on all three OSes, but fails bounded DOM and lacks
native interactive runtime evidence. Per the selection rule, neither frontend
may be recommended, and no third comparison was opened.

V03 passes: Linux functional/fault, 250,000-checkpoint, and completed
1,000,000-checkpoint results pass; native 10k smoke passes on all three OSes;
and one complete repository continues Linux → Windows → macOS with all
interchange invariants enforced. Select `git2`. V04 also passes: the Linux
20-million-word SQLite FTS5 run passes semantics, integrity, rebuild,
concurrency, and latency, and native Linux/Windows/macOS parity runs pass with
the identical corpus manifest. Select SQLite FTS5.

| Probe | Disposition | Decisive evidence |
|---|---|---|
| V01 GPUI native | Fail | No one geometry authority; no real IME/a11y bridge; ≥280.356 ms first editable pipeline; Win/mac interactive absent |
| V02 Tauri control | Fail | 13 tests and all native packages pass; unbounded baseline DOM; runtime input/a11y absent |
| V03 git2 | Pass | Linux 250k/1M and all-OS 10k pass; enforced same-repository chain passes |
| V04 SQLite FTS5 | Pass | Linux 20M passes at 11.979 ms worst first-result p99; Win/mac native parity passes |

## Exact locks

- Rust validation lock:
  `84f440580af8d156e19932f8006aa0cc49f11f7c2985283584fe952915499c77`
  (769 packages).
- V02 Rust lock:
  `c459d31ef0717bde10fd366a4151d4f781984284dc33d135c41d9eadea51f2c9`
  (394 packages).
- V02 npm lock:
  `43adb95f615d22d073973b54d5e6cfec5ac96e350edd454c0cd74158f9a71a83`
  (165 package records).

No dependency candidate is patched or forked. npm advisory scanning is clean;
Rust advisory/license automation is unavailable and recorded as unresolved.
The MPL-2.0 source-offer/notice plan is workable but not accepted.

## Required architecture change

Update `02-parchmint-architecture.md` only after architecture review:

1. Set the frontend decision to unresolved/none; record V01's geometry/input
   failure and V02's bounded-DOM/native-runtime blockers.
2. Keep ParchMint-owned semantic fixtures and restricted HTML as the durable
   boundary; neither ProseMirror JSON nor `text-document` state is canonical.
3. Select `git2 =0.21.0` for history and bundled SQLite FTS5 for search.
4. Add exact lock/provenance, static-zlib history builds, dedicated SQLite
   worker ownership, and the unresolved MPL/advisory controls.
5. Do not begin a third frontend, `gix`, or Tantivy comparison without the
   concrete failure and approval required by plan 05.

The single smallest next frontend experiment is a bounded ProseMirror DOM
strategy on `doc-large`; run the interactive native-runtime matrix only if that
passes. V01 should not continue unless an upstream-quality one-geometry
input/accessibility bridge is approved.
