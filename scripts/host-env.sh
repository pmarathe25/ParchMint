#!/usr/bin/env bash

# Source this file from Bash or Zsh after running install-dependencies.sh.

if [[ -n ${BASH_VERSION:-} ]]; then
  _parchmint_env_source=${BASH_SOURCE[0]}
elif [[ -n ${ZSH_VERSION:-} ]]; then
  _parchmint_env_source=${(%):-%N}
else
  printf 'error: scripts/host-env.sh supports Bash and Zsh\n' >&2
  return 1 2>/dev/null || exit 1
fi

_parchmint_repo_root="$(
  CDPATH= cd -- "$(dirname -- "$_parchmint_env_source")/.." && pwd
)"
_parchmint_tools_bin="$_parchmint_repo_root/.toolchains/tools/bin"
_parchmint_qt_root="$_parchmint_repo_root/.toolchains/qt/6.8.3/host"

if [[ ! -x $_parchmint_tools_bin/just || ! -x $_parchmint_qt_root/bin/qmake ]]; then
  printf 'error: ParchMint dependencies are missing; run scripts/install-dependencies.sh first\n' >&2
  unset _parchmint_env_source _parchmint_repo_root _parchmint_tools_bin _parchmint_qt_root
  return 1 2>/dev/null || exit 1
fi

export PARCHMINT_QT_ROOT="$_parchmint_qt_root"
export QT_DIR="$PARCHMINT_QT_ROOT"
export QMAKE="$PARCHMINT_QT_ROOT/bin/qmake"

case ":${CMAKE_PREFIX_PATH:-}:" in
  *":$PARCHMINT_QT_ROOT:"*) ;;
  *) export CMAKE_PREFIX_PATH="$PARCHMINT_QT_ROOT${CMAKE_PREFIX_PATH:+:$CMAKE_PREFIX_PATH}" ;;
esac
case ":${LD_LIBRARY_PATH:-}:" in
  *":$PARCHMINT_QT_ROOT/lib:"*) ;;
  *) export LD_LIBRARY_PATH="$PARCHMINT_QT_ROOT/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" ;;
esac

for _parchmint_path in "$HOME/.cargo/bin" "$PARCHMINT_QT_ROOT/bin" "$_parchmint_tools_bin"; do
  case ":$PATH:" in
    *":$_parchmint_path:"*) ;;
    *) PATH="$_parchmint_path:$PATH" ;;
  esac
done
export PATH

unset _parchmint_env_source _parchmint_repo_root _parchmint_tools_bin \
  _parchmint_qt_root _parchmint_path
