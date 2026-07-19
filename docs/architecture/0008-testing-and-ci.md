# ADR-0008: Testing framework and cross-platform CI

Status: Accepted (Stage 01)

## Decision

Use Rust unit/integration/golden tests for Qt-free layers, Qt Test for editor and
adapter behavior, QML linting plus offscreen application smoke tests for the UI,
and explicit release-mode spike benchmarks. CI runs Windows, macOS, and Linux
against Rust 1.97.1 and Qt 6.8.3.

Feature stages add tests beside their owning code. Nightly stress/fuzz work can
expand the matrix, but Stage 08 cannot substitute for missing feature tests.
Platform IME, screen-reader, and packaging charters remain manual evidence until
Stage 09 automation or release hardware is available.
