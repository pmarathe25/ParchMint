#!/usr/bin/env bash
# Detect GPU on the Docker host and write docker-compose.override.yml
# so the devcontainer gets hardware-accelerated rendering.
#
# Run once per machine (or whenever hardware changes):
#   bash scripts/detect-gpu.sh
#
# The generated file is git-ignored.

set -euo pipefail

DEVCONTAINER_DIR="$(cd "$(dirname "$0")/../.devcontainer" && pwd)"
OVERRIDE="$DEVCONTAINER_DIR/docker-compose.override.yml"

nvidia() {
  command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi >/dev/null 2>&1
}

dri() {
  [ -d /dev/dri ]
}

if nvidia; then
  cat >"$OVERRIDE" <<'EOF'
services:
  parchmint:
    deploy:
      resources:
        reservations:
          devices:
            - driver: nvidia
              count: all
              capabilities: [gpu]
EOF
  echo "NVIDIA GPU detected → wrote $OVERRIDE"

elif dri; then
  cat >"$OVERRIDE" <<'EOF'
services:
  parchmint:
    devices:
      - /dev/dri:/dev/dri
EOF
  echo "Intel/AMD GPU detected → wrote $OVERRIDE"

else
  cat >"$OVERRIDE" <<'EOF'
# No GPU detected — software rendering will be used.
services:
  parchmint: {}
EOF
  echo "No GPU detected → wrote $OVERRIDE (software rendering)"
fi
