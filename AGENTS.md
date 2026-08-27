# Repository Guidelines

## Project Structure & Module Organization

ParchMint is a Rust 2024 workspace for a local-first desktop writing application. Production code lives in `crates/`; each `parchmint-*` crate owns one boundary described in its `README.md`. The `parchmint-desktop` crate assembles services and builds the `parchmint` executable. Shared integration and visual tooling lives in `tests/`, repository policy commands live in `tools/parchmint-ci`, and release inputs live in `packaging/` and `supply-chain/`. Treat `docs/product/` as the behavior authority, `docs/architecture/` as the ownership authority, and `docs/ui-design/` as the presentation authority. `third_party/` contains the patched Iced renderer; do not edit it casually.

## Build, Test, and Development Commands

- `cargo run --locked -p parchmint-desktop --bin parchmint` launches the desktop application.
- `cargo check --workspace --all-targets --locked` checks every target without building release artifacts.
- `cargo test --workspace --locked` runs the full workspace test suite.
- `cargo test -p parchmint-ui-driver --locked -j 1 -- --test-threads=1` runs the headless end-to-end user flow.
- `cargo fmt --all --check` verifies formatting; run `cargo fmt --all` to apply it.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` enforces the CI lint policy.
- `cargo parchmint-ci architecture verify` validates crate dependency boundaries.
- `cargo parchmint-ci verify` checks repository exceptions and bundled artifacts.

Use the pinned toolchain from `rust-toolchain.toml`. CI performs locked, offline checks on Linux, macOS, and Windows after fetching dependencies.

This workstation has limited memory. Run local Cargo builds with one job, and do not run independent compilation commands in parallel. For large local checks, set `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, and `CARGO_PROFILE_TEST_DEBUG=0` to limit memory use and `target/` growth.

## Coding Style & Naming Conventions

Let `rustfmt` control four-space indentation and layout. Use `snake_case` for modules, functions, and tests; `PascalCase` for types and traits; and `SCREAMING_SNAKE_CASE` for constants. Crate directories and package names use the `parchmint-*` pattern. Workspace lints forbid unsafe code and deny all Clippy warnings. Preserve crate boundaries and reuse existing contracts and test helpers before adding dependencies or abstractions.

## Testing Guidelines

Place focused unit tests beside implementation code. Name colocated contract modules `*_contract_tests.rs`, integration tests under a crate's `tests/`, and platform-backed suites `native_*.rs`. Use descriptive behavior names such as `comparison_accepts_identical_images`. UI reference images belong in `tests/parchmint-ui-verification/references/`; changes require deliberate visual review. The headless driver in `tests/parchmint-ui-driver/` covers cross-boundary user behavior without OS windows. When it finds a regression, use its trace, observations, and diagnostics to reproduce the lowest responsible boundary in a focused unit or contract test. Keep an end-to-end scenario only when rendering, focus, event routing, or cross-boundary integration is part of the behavior. No numeric coverage target is documented, so cover changed behavior and regressions directly.

## Commit & Pull Request Guidelines

Recent commits use concise, imperative summaries, sometimes with a scope such as `docs:`. Keep each commit focused; examples include `Refine UI fidelity and headless rendering` and `docs: refresh crate documentation`. Pull requests should explain the user-visible and architectural impact, link relevant issues, list commands run, and include before/after screenshots for UI changes. Call out updates to schemas, dependencies, packaging, or supply-chain exceptions explicitly.
