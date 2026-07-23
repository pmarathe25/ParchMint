# Development environment

> Read when preparing a machine or diagnosing tool discovery.

## Ubuntu setup (recommended)

On Ubuntu 24.04 or newer, the repository installer provisions the system
libraries and pinned development tools:

```sh
./scripts/install-dependencies.sh
source scripts/host-env.sh
just bootstrap
just test
just run
```

The installer supports x86-64 and ARM64. It uses `apt` for the compiler, CMake,
and Qt runtime libraries; rustup for the version in `rust-toolchain.toml`; and
`.toolchains/` for Qt 6.8.3, `aqtinstall`, and `just` 1.50.0. It is safe to
rerun, does not change the default Rust toolchain, and does not modify shell
startup files. Run it as your normal user—not with `sudo`—and source
`scripts/host-env.sh` from Bash or Zsh in each new development shell.

Use `--dry-run` to inspect changes or `--skip-apt` when the system packages are
already managed elsewhere. The installer stops instead of overwriting an
incomplete or unexpected Qt directory.

ParchMint runs directly in the host desktop session, so Qt uses the host's
Wayland or X11 connection and GPU access without display forwarding. Files are
also created as the logged-in user and retain normal host permissions.

Node.js, Codex, and OpenCode are not project dependencies and are intentionally
not installed. Install those personal tools on the host through their own
upstream instructions; their existing shell configuration, Git configuration,
and authentication remain available when the repository is opened locally in
Zed or another editor.

The first build needs network access for locked Rust crates and CMake's pinned
CXX-Qt source dependency.

## Manual and non-Ubuntu setup

ParchMint uses pinned Rust and Qt versions so local, CI, and release behavior
agree. Install tools from their maintained upstream instructions:

| Tool | Required version or configuration | Install instructions |
|---|---|---|
| Rust | Selected automatically by `rust-toolchain.toml` | [Install Rust with rustup](https://rust-lang.org/install.html) |
| CMake | 3.24 or newer | [CMake downloads](https://cmake.org/download/) |
| Qt | 6.8.3 desktop kit with Core, Gui, Qml, Quick, Quick Controls 2, Sql, Test, and QuickTest; LinguistTools is optional | [Qt 6.8 installation guide](https://doc.qt.io/qt-6.8/gettingstarted.html) (GUI); [aqtinstall CLI](https://aqtinstall.readthedocs.io/en/stable/getting_started.html): `pip install aqtinstall` then `aqt install-qt -O ~/Qt linux desktop 6.8.3 linux_gcc_64 -m qtshadertools` (core modules ship in the base package; installs to `~/Qt/`) |
| `just` | 1.50.0 | [`just` installation options](https://just.systems/man/en/packages.html) |
| C++ compiler | C++20 compiler matching the Qt kit | [MSVC Build Tools](https://learn.microsoft.com/en-us/cpp/build/building-on-the-command-line), [Xcode command-line tools](https://developer.apple.com/documentation/xcode/installing-the-command-line-tools), or your Linux distribution's GCC/Clang package |

CXX-Qt 0.9.1 and Rust dependencies come from the locked workspace; do not
install them globally. Qt must be dynamically linked. On Windows, choose the
MSVC Qt kit and run commands from its matching developer shell. On macOS,
prefer the arm64 kit on Apple silicon.

## Make Qt discoverable

Point CXX-Qt and CMake at the same kit. An aqtinstall
`linux_gcc_64` target is stored in a directory named `gcc_64`:

```sh
export QT_DIR="$HOME/Qt/6.8.3/gcc_64"
export QMAKE="$QT_DIR/bin/qmake"
export CMAKE_PREFIX_PATH="$QT_DIR${CMAKE_PREFIX_PATH:+:$CMAKE_PREFIX_PATH}"
export LD_LIBRARY_PATH="$QT_DIR/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export PATH="$QT_DIR/bin:$PATH"
```

Use the equivalent environment-variable syntax in PowerShell. Do not mix a Qt
kit built for one compiler or architecture with another toolchain.

## Verify and build

From the repository root after activating or manually configuring the tools:

```sh
just bootstrap
just build
just test
just run
```

`just bootstrap` prints the detected Rust, CMake, and Qt versions. The exact Qt
check happens during CMake configuration. Useful narrower commands are:

```sh
just format-check
just lint
just test-rust
just build-rust
just smoke
just package-smoke
```

Rust-only commands verify Qt-free layers but are not a complete bridge or UI
gate. See [testing](testing.md) for command routing and
[coding conventions](conventions.md) for architecture guardrails.
