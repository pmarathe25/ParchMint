# Development environment

> Read when preparing a machine or diagnosing tool discovery.

## Dev container (recommended)

The dev container provisions the pinned Rust toolchain, CMake, Qt 6.8.3,
`just`, Node.js for Zed's remote agent integrations, Codex, and OpenCode. It
uses the maintained Ubuntu 24.04 C++ Dev Container image and runs as its
non-root `vscode` user. On native Linux, Dev Containers remaps that user's
numeric UID/GID to the host user so files created in the bind-mounted workspace
remain writable outside the container.

**VS Code:** Open the repository and select "Reopen in Container" when prompted,
or use the Command Palette (`Dev Containers: Reopen in Container`). The base
image includes Zsh and Oh My Zsh. To install personal shell configuration, use
VS Code's `dotfiles.repository` user setting rather than mounting the entire
host home directory.

**Zed:** Run `Project: Open Remote`, select the Dev Container option, and open
this repository. Zed does not automatically rebuild after dev-container
configuration changes; stop the existing container and reopen the project to
apply them. The image provides system Node.js so Zed's npm-based external agent
adapters do not depend on a separately downloaded runtime.

**CLI:** Install the [devcontainers CLI](https://containers.dev/cli), then:

```sh
devcontainer build --workspace-folder .
devcontainer up --workspace-folder .
devcontainer exec --workspace-folder . zsh
```

The host-side `scripts/detect-gpu.sh` initialization step generates the ignored
`.devcontainer/docker-compose.override.yml`. On Linux it:

- mounts the live host runtime directory for Wayland, Xauthority, the desktop
  session bus, and the SSH agent, with X11/XWayland socket fallback;
- applies the current host session paths to Zed, VS Code, and `devcontainer`
  processes through attach-time environment variables;
- passes `/dev/dri` and its numeric device groups, or requests an NVIDIA GPU;
- selects Qt Quick software rendering when no usable GPU device is detected;
- mounts existing Git, SSH, Codex, and OpenCode state into their correct
  container locations.

The runtime directory is mounted at the same absolute path inside the container,
so GNOME can rotate its session-specific Xwayland authority file without leaving
Docker with a stale file bind. The cookie is read directly from the live host
file and is never copied into the repository.

The base Compose file uses persistent named volumes when host Codex/OpenCode
directories do not exist. Codex keyring credentials cannot be forwarded as
files; if `codex login status` reports no session inside the container, run:

```sh
codex login --device-auth
```

For OpenCode, use `/connect` in its TUI when `opencode auth list` has no desired
provider. Both tools then retain their authentication across container rebuilds.
Set `PARCHMINT_SHARE_HOST_STATE=0` while opening or starting the container to
use only the named-volume state instead of host AI/Git/SSH state.

Display forwarding is currently supported for a local Linux container host.
Headless checks still work elsewhere with Qt's `offscreen` platform. To force
software rendering during initialization, set
`PARCHMINT_FORCE_SOFTWARE_RENDERING=1`.

Qt is installed under `/opt/qt/`; its tools and all other provisioned commands
are on `PATH`. Verify the environment and launch the application with:

```sh
just bootstrap
just build
just run
```

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
