# Development dependency inventory

The final distribution license is unresolved. This inventory is engineering
input, not permission to publish binaries. CI checks registry sources and known
advisories; Stage 09 must generate the complete transitive notices artifact.

| Dependency | Pinned/locked version | Role | Upstream license posture |
|---|---:|---|---|
| Qt | 6.8.3 dynamic | Native UI, editor, platform integration | Open-source/commercial dual licensing; dynamic-link and notice constraints require final review |
| CXX-Qt | 0.9.1 | Typed Rust/Qt bridge | MIT OR Apache-2.0 |
| Rust | 1.97.1 | Domain/application implementation | Rust project license terms |
| pulldown-cmark | Cargo.lock | CommonMark/GFM parser events and ranges | MIT |
| rusqlite + bundled SQLite | Cargo.lock | Disposable FTS5 cache spike | MIT; SQLite public domain |
| serde / noyalib | Cargo.lock / 0.0.15 | Boundary DTOs and bounded pure-Rust YAML 1.2 front matter | MIT OR Apache-2.0 |
| tempfile | Cargo.lock | Same-directory atomic replacement | MIT OR Apache-2.0 |
| tracing | Cargo.lock | Structured Rust diagnostics hooks | MIT |

No dependency authorizes network access at runtime. ParchMint version 1 contains
no network client path.
