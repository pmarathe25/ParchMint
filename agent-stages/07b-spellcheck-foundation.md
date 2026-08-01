# S65 — Cross-Platform Spellcheck Foundation

## Goal

Select and prove an offline `SpellcheckService` before broad feature waves.

## Tasks

1. Evaluate native-webview and custom/offline-engine options against the full contract.
2. Freeze identical v1 supported languages across all platforms.
3. Implement/prove project/global dictionaries, normalization, persistence, invalidation.
4. Implement/prove token-level ranked suggestions and application-owned anchored menu.
5. Implement viewport/recent-change checking, cancellation, stale-revision rejection.
6. Audit licenses, source provenance, package size, offline behavior.
7. Decide/document whether native webview spellcheck is disabled.
8. Implement selected adapter and shared contract suite.
9. Run ordinary/250k release-mode native tests on WebView2/WKWebView/WebKitGTK.

## Pass criteria

Same language inventory/semantics, dictionaries, suggestions/menu/decorations, accessibility, offline behavior, performance/memory, nonblocking failure, and supply-chain gates pass on all platforms.

If no strategy passes, stop at G20. Do not silently ship inconsistent native spellcheck or add per-document language UI.
