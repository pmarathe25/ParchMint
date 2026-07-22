# Development environment

> Read when preparing a machine or diagnosing tool discovery.

## Dev container (recommended)

The fastest way to get a working environment is the dev container, which
provisions Rust, CMake, Qt 6.8.3, and `just` automatically.

**VS Code:** Open the repository and select "Reopen in Container" when prompted,
or use the Command Palette (`Dev Containers: Reopen in Container`).

**CLI:** Install the [devcontainers CLI](https://containers.dev/cli), then:

```sh
devcontainer build --workspace-folder .
devcontainer up --workspace-folder .
devcontainer exec --workspace-folder . bash
```

The container installs Qt to `/opt/qt/`. All tools are on `PATH` inside the
container; no further environment setup is needed.

X11 forwarding is configured automatically. On Linux, ensure `xhost` permits
the container (typically works out of the box). On macOS, install
[XQuartz](https://www.xquartz.org/) and log out/in after enabling
"Allow connections from network clients" in its settings.

## Manual setup

ParchMint uses pinned Rust and Qt versions so local, CI, and release behavior
agree. Install tools from their maintained upstream instructions:

| Tool | Required version or configuration | Install instructions |
|---|---|---|
| Rust | Selected automatically by `rust-toolchain.toml` | [Install Rust with rustup](https://rust-lang.org/install.html) |
| CMake | 3.24 or newer | [CMake downloads](https://cmake.org/download/) |
| Qt | 6.8.3 desktop kit with Core, Gui, Qml, Quick, Quick Controls 2, Sql, Test, and QuickTest; LinguistTools is optional | [Qt 6.8 installation guide](https://doc.qt.io/qt-6.8/gettingstarted.html) (GUI); [aqtinstall CLI](https://aqtinstall.readthedocs.io/en/stable/getting_started.html): `pip install aqtinstall` then `aqt install-qt -O ~/Qt linux desktop 6.8.3 linux_gcc_64 -m qtshadertools` (core modules ship in the base package; installs to `~/Qt/`) |
| `just` | Current packaged release | [`just` installation options](https://just.systems/man/en/packages.html) |
| C++ compiler | C++20 compiler matching the Qt kit | [MSVC Build Tools](https://learn.microsoft.com/en-us/cpp/build/building-on-the-command-line), [Xcode command-line tools](https://developer.apple.com/documentation/xcode/installing-the-command-line-tools), or your Linux distribution's GCC/Clang package |

CXX-Qt 0.9.1 and Rust dependencies come from the locked workspace; do not
install them globally. Qt must be dynamically linked. On Windows, choose the
MSVC Qt kit and run commands from its matching developer shell. On macOS,
prefer the arm64 kit on Apple silicon.

## Make Qt discoverable

If the Qt kit's `qmake` is already on `PATH`, no extra configuration is needed.
Otherwise point CXX-Qt and CMake at the same kit:

```sh
export QMAKE="$HOME/Qt/6.8.3/linux_gcc_64/bin/qmake"
export CMAKE_PREFIX_PATH="$HOME/Qt/6.8.3/linux_gcc_64"
```

Use the equivalent environment-variable syntax in PowerShell. Do not mix a Qt
kit built for one compiler or architecture with another toolchain.

## Verify and build

From the repository root (works in both dev container and manual setup):

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
