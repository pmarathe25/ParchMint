# V02-R executive summary

**Decision: `frontend: none`.** The exact-locked Tauri 2.11.5/ProseMirror candidate cannot be recommended.

The lock precondition passed unchanged:

- `Cargo.lock`: `c459d31ef0717bde10fd366a4151d4f781984284dc33d135c41d9eadea51f2c9`
- `package-lock.json`: `43adb95f615d22d073973b54d5e6cfec5ac96e350edd454c0cd74158f9a71a83`

On Ubuntu 26.04 with WebKitGTK 2.52.3, the exact deb payload launched natively. Its packaged `tauri://localhost` loader still produced:

```text
Unavailable: SyntaxError: The string did not match the expected pattern.. Run npm run fixtures:sync.
```

The user correctly reported that the viewport itself works. A corrected native
WebKitGTK retry against the exact-locked V02 Vite content loaded the accepted
fixture repeatedly. The probe reported 248,079 adapted words, 1,923 mounted
blocks in each view, and first-editable-viewport samples of 640, 637, and
639 ms. Thus the earlier “no viewport” characterization was wrong for the
development viewport. The measured result nevertheless fails the mandatory
≤250 ms gate. It does not cure the packaged custom-scheme loader defect and
cannot be promoted to packaged-Tauri evidence because `environment.tauri` was
`false`.

One non-isolated diagnostic input trace recorded 123 samples at
p50/p95/p99 `119/861/1012 ms`, with view-to-view propagation
`2/5/8 ms`. Clean isolated retries could not place native focus in the
contenteditable through AT-SPI/XTEST and therefore produced no acceptance
input distribution. Those numbers are preserved as diagnostic raw evidence,
not a hard-gate pass. The exact AppImage also failed to launch on this host
because of bundled GLib/GVFS incompatibility followed by `EGL_BAD_PARAMETER`.

Windows and macOS retain exact package-build evidence but were not installed or launched on native interactive hosts. WebView2/WKWebView versions, real input, screen readers, high DPI, and runtime measurements are unknown and therefore fail the mandatory all-platform rule.

Diagnostic Linux evidence was preserved rather than promoted to a pass:

- 59 one-second process-tree samples in the corrected two-view development viewport: RSS `410120 → 482276 KiB`, range `408984–491816 KiB`, private dirty peak `157604 KiB`, CPU peak `59.940%`, and swap `0 KiB`. This is a 60-second characterization, not the required long-run/view-close memory pass.
- Native XTEST and Wayland-clipboard attempts included plain/rich/Paste Without Formatting, combining marks, emoji, Arabic, literal Tab, undo/redo, and context menu. Clean acceptance in both editors and CJK IME remained unconfirmed.
- The AT-SPI tree exposed both editors with Accessible, Component, Text, Action, and Collection interfaces. No Orca editing transcript was obtained, so native screen-reader usability is not established.
- External canonical-save, word-count, accepted `git2`, and SQLite FTS5 contention jobs completed while `doc-large` was mounted, but the browser harness has no Tauri IPC and the V02 binary contains checksum stand-ins; these results are diagnostic only.

Accepted semantic evidence remains intact: 13/13 V02 tests pass and all four canonical before/after fixtures are byte-identical. Neither result substitutes for native runtime acceptance.

The required next action is a product-owner choice among the three authorized options:

1. Lower the guaranteed maximum individual-document size.
2. Add a constrained or segmented mode for exceptionally large documents.
3. Fund a substantially custom editor/view implementation.

This package does not choose among them and does not open a third frontend comparison. `git2` and SQLite FTS5 remain the selected history and search backends.
