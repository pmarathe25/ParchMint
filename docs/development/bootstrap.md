# Developer bootstrap

## Pinned toolchain

- Rust 1.97.1, installed through `rust-toolchain.toml`
- CMake 3.24 or newer
- Qt 6.8.3 with Core, Gui, Qml, Quick, Quick Controls 2, Sql, Test, and QuickTest
- CXX-Qt 0.9.1
- `just`
- A C++20 compiler supported by Qt

Qt must be a dynamically linked installation. Set `QMAKE` to the Qt 6.8.3
`qmake` executable when it is not on `PATH`. CXX-Qt uses it to locate the exact
Qt installation. On Windows, use the MSVC Qt build and matching Visual Studio
developer shell. On macOS, an arm64 Qt build is preferred on Apple silicon.

The Linux development environment used for Stage 01 is retained locally in the
git-ignored `.toolchains/` directory. From the repository root, activate it with:

```sh
export PARCHMINT_QT_ROOT="$PWD/.toolchains/qt/6.8.3/gcc_64"
export QMAKE="$PARCHMINT_QT_ROOT/bin/qmake"
export CMAKE_PREFIX_PATH="$PARCHMINT_QT_ROOT"
export LD_LIBRARY_PATH="$PARCHMINT_QT_ROOT/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export PATH="$PWD/.toolchains/tools/bin:$PARCHMINT_QT_ROOT/bin:$PATH"
```

The directory is a machine-local convenience, not a portable or committed
dependency. Developers on other systems should install the pinned Qt version
using the platform guidance above.

## Stable commands

```sh
just bootstrap
just format-check
just lint
just test
just build
just run
just smoke
just package-smoke
```

`just build-rust` and `just test-rust` verify the Qt-free layers when Qt is not
installed. This is useful for storage/domain work, but it is not the Stage 01
acceptance build.

The application writes local JSON-lines diagnostics to the platform application
data directory. It performs no upload and has no network code. A user must
explicitly copy or export diagnostics before another person can inspect them.

## Architecture guardrails

- Add domain behavior below `crates/parchmint-bridge`; never add Qt types to a
  domain crate.
- QML invokes typed bridge APIs and never opens project paths itself.
- Use `parchmint_storage::atomic_write` for future canonical replacements.
- Associate background operations with `WorkStamp` and validate the generation
  and revision before applying completion.
- Never use `QTextDocument::toMarkdown()` as canonical persistence.
