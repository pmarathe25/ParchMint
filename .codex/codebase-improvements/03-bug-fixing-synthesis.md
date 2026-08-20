# Bug-fixing synthesis

The Stage 3 reviews confirmed eight behavior-affecting defects. The diagnostics
symlink bug is already fixed. Implement the remaining findings by subsystem and
keep the uncertain observations out of scope until a test proves them.

## Confirmed fixes

1. **Diagnostics log target:** configuration could follow and truncate a
   symlink target. Fixed with no-follow platform opens and regression tests.
2. **Recovery frontier:** acknowledging or replaying a partial document batch
   replaces the full revision frontier. Merge touched document revisions in all
   three paths and test alternating updates to two documents.
3. **Project completeness:** `decode_project` and migration accept a format
   marker without `project.toml`. Reject missing required resources.
4. **Repository lease:** the in-memory repository serves documents after its
   `OpenProject` lease is dropped. Require a live lease for document loads.
5. **Project dictionary isolation:** spellcheck lookup keys project words only
   by revision, so projects with equal revisions share private words. Include
   the project ID in lookup and suggestion selection.
6. **CSS sanitization:** escaped or comment-obfuscated CSS bypasses literal
   checks for dangerous URL and expression constructs. Normalize or parse CSS
   and reject unsafe forms before export.
7. **Window authority:** native platform work checks a window before dispatch,
   but not immediately before its OS side effect. Reauthorize inside the worker.
8. **Desktop close:** close paths unregister the project before the UI confirms
   closure. Keep the session registered when the callback fails so retry works.

## Deferred observations

- Search cancellation generations may accumulate or remain canceled if callers
  reuse an ID; the API does not promise reuse.
- Concurrent native menu installs may update backend bookkeeping out of order;
  no user-visible failure is established.
- The architecture guard scans product crates under `crates/`, not every
  workspace member; its intended policy scope needs confirmation.

The detailed evidence and regression guidance are in `03-bug-fixing.md` and
`03a` through `03e` reports in this directory.
