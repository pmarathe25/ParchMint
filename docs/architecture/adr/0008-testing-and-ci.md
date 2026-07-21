# ADR-0008: Testing framework and cross-platform CI

Status: Accepted

## Decision

Use Rust unit/integration/golden tests for Qt-free layers, Qt Test for editor and
adapter behavior, QML linting plus offscreen application smoke tests for the UI,
and explicit release-mode spike benchmarks. CI runs Windows, macOS, and Linux
against Rust 1.97.1 and Qt 6.8.3.

Feature changes add tests beside their owning code. Nightly stress/fuzz work
expands the matrix but cannot substitute for missing feature tests. Platform
IME, screen-reader, and packaging charters remain manual release evidence where
automation cannot reproduce physical behavior.
