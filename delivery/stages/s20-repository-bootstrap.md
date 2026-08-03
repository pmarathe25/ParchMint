# S20 — Repository Bootstrap and Governance

## Goal

Create the reproducible application workspace and CI only. Do not implement product features or final UI.

## Tasks

1. Create architecture monorepo layout.
2. Pin Rust/Node/package manager/Tauri/React/TypeScript and build tools.
3. Apply direct dependency baseline and create actual application locks.
4. Assert `git2`/vendored libgit2/static-zlib and rusqlite/bundled SQLite composition from real resolution.
5. Pin initial ProseMirror direct versions and verify npm integrity/source metadata.
6. Bootstrap empty Tauri/React shell and headless CLI.
7. Prove one generated/validated Rust↔TypeScript JSON Schema round trip.
8. Add contract generation plus CI dirty-diff guard.
9. Add format/lint/typecheck/test/build/package commands.
10. Add Windows/macOS/Linux CI.
11. Add advisory/license/provenance/SBOM/native-notice tooling and weekly scheduled checks.
12. Add deterministic fixture/checksum tooling and developer setup.
13. Commit a machine-readable supply-chain policy covering licenses, advisory thresholds, provenance changes, bounded exceptions, and hashes for bundled native/font/dictionary artifacts.
14. Commit a deny-by-default Tauri threat model/capability matrix; bind privileged commands to server-verified window/project sessions and scoped path handles.
15. Build and launch a minimal packaged release artifact on Windows, macOS, and Linux; prove bundled asset load and one privileged IPC round trip without a development server.

## Pass criteria

- Clean builds on all three platforms.
- Actual locks and dependency inventory committed.
- Composition/provenance assertions pass.
- One contract round trip and clean generated diff.
- Packaged release smoke passes on all three platforms with evidence labelled separately from development-webview checks.
- CSP/capability/navigation/session-isolation and supply-chain policy checks pass.
- No feature behavior/design interpretation.

Do not create a bootstrap architecture-decision record or changelog; the current architecture document is authoritative.
