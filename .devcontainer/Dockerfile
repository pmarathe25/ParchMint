FROM mcr.microsoft.com/devcontainers/cpp:ubuntu-24.04

ARG DEBIAN_FRONTEND=noninteractive
ARG TARGETARCH
ARG QT_VERSION=6.8.3
ARG AQTINSTALL_VERSION=3.3.0
ARG JUST_VERSION=1.50.0
ARG CODEX_VERSION=0.144.6
ARG OPENCODE_VERSION=1.18.4

USER root

# Build tools plus the complete runtime set used by Qt's XCB and Wayland QPA
# plugins. The base image already provides Git, sudo, Zsh, and a non-root
# `vscode` user suitable for Dev Containers' UID/GID remapping.
RUN sed -i \
      -e 's|http://security.ubuntu.com/ubuntu|https://archive.ubuntu.com/ubuntu|g' \
      -e 's|http://archive.ubuntu.com/ubuntu|https://archive.ubuntu.com/ubuntu|g' \
      /etc/apt/sources.list.d/ubuntu.sources \
    && apt-get -o Acquire::ForceIPv4=true update \
    && apt-get -o Acquire::ForceIPv4=true install -y --no-install-recommends \
    ca-certificates \
    cmake \
    curl \
    file \
    fonts-dejavu-core \
    libdbus-1-3 \
    libdecor-0-0 \
    libegl1 \
    libfontconfig1 \
    libfreetype6 \
    libgl1 \
    libgl1-mesa-dri \
    libglib2.0-0t64 \
    libice6 \
    libopengl0 \
    libsm6 \
    libsqlite3-0 \
    libwayland-client0 \
    libwayland-cursor0 \
    libwayland-egl1 \
    libx11-6 \
    libx11-xcb1 \
    libxcb-cursor0 \
    libxcb-icccm4 \
    libxcb-image0 \
    libxcb-keysyms1 \
    libxcb-randr0 \
    libxcb-render-util0 \
    libxcb-render0 \
    libxcb-shape0 \
    libxcb-shm0 \
    libxcb-sync1 \
    libxcb-util1 \
    libxcb-xfixes0 \
    libxcb-xinerama0 \
    libxcb-xinput0 \
    libxcb-xkb1 \
    libxcb1 \
    libxext6 \
    libxi6 \
    libxkbcommon-x11-0 \
    libxkbcommon0 \
    libxrender1 \
    locales \
    mesa-utils \
    mesa-vulkan-drivers \
    ninja-build \
    pkg-config \
    python3 \
    python3-pip \
    x11-utils \
    && rm -rf /var/lib/apt/lists/*

# Install the pinned Qt SDK for the container architecture and expose a stable
# path regardless of whether the kit is x86-64 or ARM64.
RUN python3 -m pip install --break-system-packages "aqtinstall==${AQTINSTALL_VERSION}" \
    && architecture="${TARGETARCH:-$(dpkg --print-architecture)}" \
    && case "${architecture}" in \
         amd64) qt_arch=linux_gcc_64; qt_dir=gcc_64 ;; \
         arm64) qt_arch=linux_gcc_arm64; qt_dir=gcc_arm64 ;; \
         *) echo "Unsupported container architecture: ${architecture}" >&2; exit 1 ;; \
       esac \
    && aqt install-qt -O /opt/qt linux desktop "${QT_VERSION}" "${qt_arch}" -m qtshadertools \
    && ln -s "/opt/qt/${QT_VERSION}/${qt_dir}" "/opt/qt/${QT_VERSION}/host" \
    && test -e "/opt/qt/${QT_VERSION}/host/plugins/platforms/libqxcb.so" \
    && test -e "/opt/qt/${QT_VERSION}/host/plugins/platforms/libqwayland-egl.so" \
    && rm -rf /opt/qt/Docs /opt/qt/Examples /opt/qt/Tools /root/.cache/pip

# The downloaded Qt SDK's CMake metadata links against the unversioned GLX
# development library. Vulkan headers also keep Qt's optional feature probes
# complete during project configuration.
RUN apt-get -o Acquire::ForceIPv4=true update \
    && apt-get -o Acquire::ForceIPv4=true install -y --no-install-recommends \
      libgl1-mesa-dev \
      libvulkan-dev \
    && rm -rf /var/lib/apt/lists/*

ENV QT_DIR=/opt/qt/${QT_VERSION}/host
ENV PATH="${QT_DIR}/bin:${PATH}"
ENV LD_LIBRARY_PATH="${QT_DIR}/lib"
ENV QMAKE="${QT_DIR}/bin/qmake"
ENV CMAKE_PREFIX_PATH="${QT_DIR}"

# Install a current system Node.js for Zed's remote agent integrations, then
# install versioned standalone command-line tools globally so home-directory
# mounts never obscure the executables or their runtime dependencies.
ARG NODE_VERSION=24.11.0
RUN architecture="${TARGETARCH:-$(dpkg --print-architecture)}" \
    && case "${architecture}" in \
         amd64) node_arch=x64 ;; \
         arm64) node_arch=arm64 ;; \
         *) echo "Unsupported container architecture: ${architecture}" >&2; exit 1 ;; \
       esac \
    && mkdir -p /opt/node \
    && curl --proto '=https' --tlsv1.2 -fsSL \
      "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${node_arch}.tar.gz" \
      | tar -xz -C /opt/node --strip-components=1 \
    && ln -sf /opt/node/bin/node /usr/local/bin/node \
    && ln -sf /opt/node/bin/npm /usr/local/bin/npm \
    && ln -sf /opt/node/bin/npx /usr/local/bin/npx \
    && node --version \
    && npm --version \
    && curl --proto '=https' --tlsv1.2 -fsSL https://just.systems/install.sh \
      | bash -s -- --to /usr/local/bin --tag "${JUST_VERSION}" \
    && curl --proto '=https' --tlsv1.2 -fsSL https://chatgpt.com/codex/install.sh \
      | CODEX_RELEASE="${CODEX_VERSION}" \
        CODEX_NON_INTERACTIVE=1 \
        CODEX_INSTALL_DIR=/usr/local/bin \
        CODEX_HOME=/opt/codex \
        sh \
    && curl --proto '=https' --tlsv1.2 -fsSL https://opencode.ai/install \
      | bash -s -- --version "${OPENCODE_VERSION}" --no-modify-path \
    && install -m 0755 /root/.opencode/bin/opencode /usr/local/bin/opencode \
    && rm -rf /root/.opencode \
    && just --version \
    && codex --version \
    && opencode --version

# Rust is pinned by rust-toolchain.toml. Install that toolchain for the
# non-root user; Dev Containers will remap this user's numeric UID/GID on Linux.
USER vscode
RUN curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain 1.97.1 --profile minimal \
        --component clippy,rustfmt \
    && mkdir -p \
      /home/vscode/.cargo/git \
      /home/vscode/.cargo/registry \
      /home/vscode/.codex \
      /home/vscode/.config/opencode \
      /home/vscode/.local/share/opencode \
      /tmp/parchmint-runtime \
    && chmod 0700 /tmp/parchmint-runtime

# Compose adds the non-root user to the host's numeric DRI groups. Give any
# host-specific GIDs missing from the image stable names so tools such as
# `groups` can resolve every supplementary group after container creation.
USER root
ARG HOST_DRI_GIDS=""
RUN set -eu; \
    for gid in ${HOST_DRI_GIDS}; do \
      case "${gid}" in *[!0-9]*|'') echo "Invalid DRI group ID: ${gid}" >&2; exit 1 ;; esac; \
      if ! getent group "${gid}" >/dev/null; then \
        groupadd --gid "${gid}" "host-dri-${gid}"; \
      fi; \
    done

USER vscode

ENV PATH="/home/vscode/.cargo/bin:${PATH}"

WORKDIR /workspaces/ParchMint

# Qt expects a UTF-8 process locale; Ubuntu provides C.UTF-8 without locale
# generation at container startup.
ENV LANG=C.UTF-8
ENV LC_ALL=C.UTF-8
