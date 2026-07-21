# Testing

> Read when selecting validation for a change or adding regression coverage.

## Command router

| Change | Minimum command |
|---|---|
| Rust formatting | `just format-check` |
| Qt-free Rust behavior | `just test-rust` |
| Rust lint/API changes | `just lint` |
| QML, C++, bridge, lifecycle | `just test` |
| Smoke/startup behavior | `just smoke` |
| Release-scale performance | `just bench-spikes` |
| Packaging rules | `just package-smoke` |

Run the narrow check while iterating, then the broad gate for the affected
boundary. Always run `git diff --check`.

## Test placement

- Rust unit and integration tests stay beside owning behavior.
- Compatibility inputs live under `tests/fixtures/` by contract area.
- Qt adapter/model tests live under `tests/qt/`.
- End-to-end startup and lifecycle paths run through CTest.
- Large corpora are generated from committed seeds; never commit generated scale data.

For documentation moves, search the repository for every old path and validate
all changed local links. For format changes, add a named compatibility fixture
and verify deterministic reopen/round-trip behavior.

Performance commands and corpora are defined in [performance](performance.md).
Physical platform checks are defined in
[release validation](../release/platform-validation.md).
