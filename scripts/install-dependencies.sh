#!/usr/bin/env bash

# Install ParchMint's native Ubuntu development dependencies. System packages
# are installed with apt; versioned project tools stay under .toolchains/.

set -euo pipefail

readonly QT_VERSION=6.8.3
readonly AQTINSTALL_VERSION=3.3.0
readonly JUST_VERSION=1.50.0
readonly MINIMUM_CMAKE_VERSION=3.24

script_dir="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/.." && pwd)"
toolchains_dir="$repo_root/.toolchains"
python_env="$toolchains_dir/python"
qt_parent="$toolchains_dir/qt"
tools_bin="$toolchains_dir/tools/bin"
toolchain_file="$repo_root/rust-toolchain.toml"

dry_run=false
skip_apt=false
cleanup_paths=()

usage() {
  cat <<'EOF'
Usage: scripts/install-dependencies.sh [OPTIONS]

Install the native dependencies needed to build and run ParchMint on Ubuntu.
The script is safe to rerun and does not change the default Rust toolchain or
modify shell startup files.

Options:
  --dry-run   Show the planned changes without modifying the machine.
  --skip-apt  Do not check or install Ubuntu packages.
  -h, --help  Show this help text.
EOF
}

log() {
  printf '\n==> %s\n' "$*"
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

print_command() {
  printf '  '
  printf '%q ' "$@"
  printf '\n'
}

cleanup() {
  local path
  for path in "${cleanup_paths[@]}"; do
    if [[ -n $path && -d $path ]]; then
      rm -rf -- "$path"
    fi
  done
}
trap cleanup EXIT

while (($#)); do
  case "$1" in
    --dry-run)
      dry_run=true
      ;;
    --skip-apt)
      skip_apt=true
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      die "unknown option: $1"
      ;;
  esac
  shift
done

((EUID != 0)) ||
  die "do not run this script with sudo; it requests sudo only for apt packages"

[[ -r /etc/os-release ]] || die "this installer requires Ubuntu 24.04 or newer"
# shellcheck disable=SC1091
source /etc/os-release
[[ ${ID:-} == ubuntu ]] || die "unsupported operating system '${ID:-unknown}'; use the manual setup guide"
command -v dpkg >/dev/null 2>&1 || die "dpkg is required"
dpkg --compare-versions "${VERSION_ID:-0}" ge 24.04 ||
  die "Ubuntu 24.04 or newer is required; found ${VERSION_ID:-unknown}"

case "$(dpkg --print-architecture)" in
  amd64)
    qt_host_os=linux
    qt_arch=linux_gcc_64
    qt_dir=gcc_64
    ;;
  arm64)
    qt_host_os=linux_arm64
    qt_arch=linux_gcc_arm64
    qt_dir=gcc_arm64
    ;;
  *)
    die "unsupported architecture: $(dpkg --print-architecture)"
    ;;
esac

read_toml_string() {
  local key=$1
  awk -F '"' -v key="$key" \
    '$1 ~ "^[[:space:]]*" key "[[:space:]]*=" { print $2; exit }' \
    "$toolchain_file"
}

rust_version="$(read_toml_string channel)"
rust_profile="$(read_toml_string profile)"
components_text="$(
  sed -nE 's/^[[:space:]]*components[[:space:]]*=[[:space:]]*\[(.*)\][[:space:]]*$/\1/p' \
    "$toolchain_file" | head -n 1
)"
rust_components=()
IFS=',' read -r -a component_fields <<<"$components_text"
for component in "${component_fields[@]}"; do
  component="${component//\"/}"
  component="$(printf '%s' "$component" | tr -d '[:space:]')"
  [[ -n $component ]] && rust_components+=("$component")
done
[[ -n $rust_version && -n $rust_profile && ${#rust_components[@]} -gt 0 ]] ||
  die "could not read the Rust toolchain settings from $toolchain_file"

apt_packages=(
  build-essential
  ca-certificates
  cmake
  curl
  file
  fonts-dejavu-core
  git
  libdbus-1-3
  libdecor-0-0
  libegl1
  libfontconfig1
  libfreetype6
  libgl1
  libgl1-mesa-dev
  libgl1-mesa-dri
  libglib2.0-0t64
  libice6
  libopengl0
  libsm6
  libsqlite3-0
  libvulkan-dev
  libwayland-client0
  libwayland-cursor0
  libwayland-egl1
  libx11-6
  libx11-xcb1
  libxcb-cursor0
  libxcb-icccm4
  libxcb-image0
  libxcb-keysyms1
  libxcb-randr0
  libxcb-render-util0
  libxcb-render0
  libxcb-shape0
  libxcb-shm0
  libxcb-sync1
  libxcb-util1
  libxcb-xfixes0
  libxcb-xinerama0
  libxcb-xinput0
  libxcb-xkb1
  libxcb1
  libxext6
  libxi6
  libxkbcommon-x11-0
  libxkbcommon0
  libxrender1
  ninja-build
  pkg-config
  python3
  python3-venv
)

missing_packages=()
if ! $skip_apt; then
  for package in "${apt_packages[@]}"; do
    package_status="$(dpkg-query -W -f='${Status}' "$package" 2>/dev/null || true)"
    [[ $package_status == "install ok installed" ]] || missing_packages+=("$package")
  done
fi

export PATH="$HOME/.cargo/bin:$PATH"
rustup_bin="$(command -v rustup || true)"
qt_version_dir="$qt_parent/$QT_VERSION"
qt_kit="$qt_version_dir/$qt_dir"
qt_host="$qt_version_dir/host"
just_bin="$tools_bin/just"

rust_toolchain_ready() {
  local component installed_components toolchains
  [[ -n $rustup_bin ]] || return 1
  toolchains="$($rustup_bin toolchain list 2>/dev/null)" || return 1
  grep -Eq "^${rust_version}(-|[[:space:]])" <<<"$toolchains" || return 1
  installed_components="$($rustup_bin component list --toolchain "$rust_version" --installed 2>/dev/null)" || return 1
  for component in "${rust_components[@]}"; do
    grep -Eq "^${component}(-|[[:space:]])" <<<"$installed_components" || return 1
  done
  "$rustup_bin" run "$rust_version" cargo --version >/dev/null 2>&1 || return 1
}

aqt_version() {
  [[ -x $python_env/bin/python ]] || return 1
  "$python_env/bin/python" -c \
    'from importlib.metadata import version; print(version("aqtinstall"))' \
    2>/dev/null
}

qt_kit_is_valid() {
  local kit=$1
  [[ -x $kit/bin/qmake ]] || return 1
  [[ $($kit/bin/qmake -query QT_VERSION 2>/dev/null) == "$QT_VERSION" ]] || return 1
  [[ -f $kit/plugins/platforms/libqxcb.so ]] || return 1
  [[ -f $kit/plugins/platforms/libqwayland-egl.so ]] || return 1
}

just_is_valid() {
  [[ -x $just_bin ]] && [[ $($just_bin --version 2>/dev/null) == "just $JUST_VERSION" ]]
}

rustup_args=(toolchain install "$rust_version" --profile "$rust_profile")
for component in "${rust_components[@]}"; do
  rustup_args+=(--component "$component")
done

if $dry_run; then
  log "Dry run for Ubuntu ${VERSION_ID} ($(dpkg --print-architecture))"
  if ! $skip_apt && ((${#missing_packages[@]})); then
    print_command sudo apt-get update
    print_command sudo apt-get install -y --no-install-recommends "${missing_packages[@]}"
  fi
  if [[ -z $rustup_bin ]]; then
    print_command curl --proto =https --tlsv1.2 -fsSL https://sh.rustup.rs -o '<temporary>/rustup-init.sh'
    print_command sh '<temporary>/rustup-init.sh' -y --profile minimal --default-toolchain none --no-modify-path
    rustup_bin="$HOME/.cargo/bin/rustup"
  fi
  rust_toolchain_ready || print_command "$rustup_bin" "${rustup_args[@]}"
  [[ -x $python_env/bin/python ]] || print_command python3 -m venv "$python_env"
  [[ $(aqt_version || true) == "$AQTINSTALL_VERSION" ]] ||
    print_command "$python_env/bin/python" -m pip install --disable-pip-version-check "aqtinstall==$AQTINSTALL_VERSION"
  if ! qt_kit_is_valid "$qt_kit"; then
    if [[ -e $qt_version_dir || -L $qt_version_dir ]]; then
      printf '  would stop: existing Qt directory is incomplete: %s\n' "$qt_version_dir"
      printf '\nNo changes were made.\n'
      exit 0
    else
      print_command "$python_env/bin/aqt" install-qt -O '<repo-local staging directory>' "$qt_host_os" desktop "$QT_VERSION" "$qt_arch" -m qtshadertools
      print_command mv '<repo-local staging directory>/6.8.3' "$qt_version_dir"
    fi
  fi
  if [[ ! -L $qt_host || $(readlink "$qt_host" 2>/dev/null || true) != "$qt_dir" ]]; then
    print_command ln -s "$qt_dir" "$qt_host"
  fi
  if ! just_is_valid; then
    print_command curl --proto =https --tlsv1.2 -fsSL https://just.systems/install.sh -o '<temporary>/install-just.sh'
    print_command bash '<temporary>/install-just.sh' --to "$tools_bin" --tag "$JUST_VERSION"
  fi
  printf '\nNo changes were made.\n'
  exit 0
fi

if ! $skip_apt && ((${#missing_packages[@]})); then
  log "Installing ${#missing_packages[@]} missing Ubuntu packages"
  command -v sudo >/dev/null 2>&1 || die "sudo is required to install Ubuntu packages"
  sudo apt-get update
  sudo apt-get install -y --no-install-recommends "${missing_packages[@]}"
elif $skip_apt; then
  log "Skipping Ubuntu packages"
else
  log "Ubuntu packages are already installed"
fi

for command_name in cmake curl git ninja python3; do
  command -v "$command_name" >/dev/null 2>&1 ||
    die "missing required command '$command_name'; rerun without --skip-apt"
done
cmake_version="$(cmake --version | awk 'NR == 1 { print $3 }')"
dpkg --compare-versions "$cmake_version" ge "$MINIMUM_CMAKE_VERSION" ||
  die "CMake $MINIMUM_CMAKE_VERSION or newer is required; found $cmake_version"

mkdir -p "$toolchains_dir" "$qt_parent" "$tools_bin"
temporary_dir="$(mktemp -d "${TMPDIR:-/tmp}/parchmint-dependencies.XXXXXXXX")"
cleanup_paths+=("$temporary_dir")

download_file() {
  local url=$1
  local destination=$2
  curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location \
    "$url" --output "$destination"
}

if [[ -z $rustup_bin ]]; then
  log "Installing rustup"
  rustup_installer="$temporary_dir/rustup-init.sh"
  download_file https://sh.rustup.rs "$rustup_installer"
  sh "$rustup_installer" -y --profile minimal --default-toolchain none --no-modify-path
  rustup_bin="$HOME/.cargo/bin/rustup"
  [[ -x $rustup_bin ]] || die "rustup installation did not create $rustup_bin"
fi

if rust_toolchain_ready; then
  log "Rust $rust_version and required components are already installed"
else
  log "Installing Rust $rust_version"
  "$rustup_bin" "${rustup_args[@]}"
fi

if [[ ! -x $python_env/bin/python ]]; then
  log "Creating the aqtinstall virtual environment"
  python3 -m venv "$python_env"
fi
if [[ $(aqt_version || true) != "$AQTINSTALL_VERSION" ]]; then
  log "Installing aqtinstall $AQTINSTALL_VERSION"
  "$python_env/bin/python" -m pip install --disable-pip-version-check \
    "aqtinstall==$AQTINSTALL_VERSION"
else
  log "aqtinstall $AQTINSTALL_VERSION is already installed"
fi

if qt_kit_is_valid "$qt_kit"; then
  log "Qt $QT_VERSION is already installed"
elif [[ -e $qt_version_dir || -L $qt_version_dir ]]; then
  die "existing Qt directory is incomplete or invalid: $qt_version_dir (move or remove it, then rerun)"
else
  log "Installing Qt $QT_VERSION with qtshadertools"
  qt_staging="$(mktemp -d "$toolchains_dir/.qt-install.XXXXXXXX")"
  cleanup_paths+=("$qt_staging")
  "$python_env/bin/aqt" install-qt -O "$qt_staging" "$qt_host_os" desktop \
    "$QT_VERSION" "$qt_arch" -m qtshadertools
  staged_kit="$qt_staging/$QT_VERSION/$qt_dir"
  qt_kit_is_valid "$staged_kit" || die "the downloaded Qt kit failed validation"
  mv "$qt_staging/$QT_VERSION" "$qt_version_dir"
fi

if [[ -e $qt_host && ! -L $qt_host ]]; then
  die "cannot create the architecture-neutral Qt link because $qt_host is not a symlink"
fi
if [[ ! -L $qt_host || $(readlink "$qt_host") != "$qt_dir" ]]; then
  [[ -L $qt_host ]] && unlink "$qt_host"
  ln -s "$qt_dir" "$qt_host"
fi

if just_is_valid; then
  log "just $JUST_VERSION is already installed"
else
  log "Installing just $JUST_VERSION"
  just_installer="$temporary_dir/install-just.sh"
  download_file https://just.systems/install.sh "$just_installer"
  bash "$just_installer" --to "$tools_bin" --tag "$JUST_VERSION"
fi

log "Validating the native toolchain"
rust_toolchain_ready || die "Rust $rust_version or one of its required components is missing"
qt_kit_is_valid "$qt_host" || die "Qt $QT_VERSION failed final validation"
just_is_valid || die "just $JUST_VERSION failed final validation"

printf '\nParchMint dependencies are ready. Activate them in this shell with:\n\n'
printf '  source %q\n\n' "$script_dir/host-env.sh"
printf 'Then run: just bootstrap && just test\n'
