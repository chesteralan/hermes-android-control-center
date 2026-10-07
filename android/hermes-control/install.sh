#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PYTHON=${PYTHON:-python3}
APP_HOME=${HERMES_CONTROL_HOME:-"$HOME/.local/share/hermes-control"}
CONFIG=${HERMES_CONTROL_CONFIG:-"$HOME/.config/hermes-control.toml"}
PREFIX=${PREFIX:-"/data/data/com.termux/files/usr"}

"$PYTHON" -c 'import sys; raise SystemExit(0 if sys.version_info >= (3, 11) else "Python 3.11 or newer is required")'
if ! command -v sv >/dev/null 2>&1; then
    if ! command -v pkg >/dev/null 2>&1; then
        printf 'Termux pkg is required to install termux-services.\n' >&2
        exit 1
    fi
    pkg install -y termux-services
fi
"$PYTHON" -m venv "$APP_HOME/venv"
"$APP_HOME/venv/bin/python" -m pip install "$ROOT"

if [ ! -f "$CONFIG" ]; then
    mkdir -p "$(dirname -- "$CONFIG")"
    cp "$ROOT/hermes-control.toml" "$CONFIG"
    chmod 600 "$CONFIG"
fi

SERVICE_DIR="$PREFIX/var/service/hermes-control"
mkdir -p "$SERVICE_DIR"
cp "$ROOT/service/hermes-control/run" "$SERVICE_DIR/run"
chmod 700 "$SERVICE_DIR/run"

printf 'Installed hermes-control. Config: %s\nService: %s\nStart with: sv up hermes-control\n' \
    "$CONFIG" "$SERVICE_DIR"
