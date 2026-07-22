FROM ubuntu:24.04

ARG DEBIAN_FRONTEND=noninteractive

# System packages for build, Qt, X11 forwarding, and GL
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    cmake \
    curl \
    file \
    git \
    libdbus-1-dev \
    libegl1-mesa-dev \
    libfontconfig1-dev \
    libfreetype-dev \
    libgl1-mesa-dev \
    libglib2.0-dev \
    libinput-dev \
    libwayland-cursor0 \
    libwayland-egl1 \
    libx11-xcb1 \
    libxcb-cursor0 \
    libxcb-xinerama0 \
    libxcb-xinput0 \
    libxkbcommon-dev \
    libxrender-dev \
    pkg-config \
    python3-pip \
    x11-utils \
    && rm -rf /var/lib/apt/lists/*

# aqtinstall — install Qt 6.8.3
RUN pip install --break-system-packages aqtinstall \
    && aqt install-qt -O /opt/qt linux desktop 6.8.3 linux_gcc_64 -m qtshadertools \
    && rm -rf /opt/qt/Docs /opt/qt/Examples /opt/qt/Tools

ENV QT_DIR=/opt/qt/6.8.3/gcc_64
ENV PATH="${QT_DIR}/bin:${PATH}"
ENV LD_LIBRARY_PATH="${QT_DIR}/lib:${LD_LIBRARY_PATH:-}"
ENV QMAKE=${QT_DIR}/bin/qmake
ENV CMAKE_PREFIX_PATH=${QT_DIR}

# Rust — pinned by rust-toolchain.toml; install the exact version directly
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain 1.97.1 --profile minimal --component clippy,rustfmt
ENV PATH="/root/.cargo/bin:${PATH}"

# just
RUN curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to /usr/local/bin

WORKDIR /workspaces/ParchMint
