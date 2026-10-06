#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PYTHON=${PYTHON:-python3}
APP_HOME=${HERMES_CONTROL_HOME:-"$HOME/.local/share/hermes-control"}
CONFIG=${HERMES_CONTROL_CONFIG:-"$HOME/.config/hermes-control.toml"}

"$PYTHON" -c 'import sys; raise SystemExit(0 if sys.version_info >= (3, 11) else "Python 3.11 or newer is required")'
"$PYTHON" -m venv "$APP_HOME/venv"
"$APP_HOME/venv/bin/python" -m pip install "$ROOT"

if [ ! -f "$CONFIG" ]; then
    mkdir -p "$(dirname -- "$CONFIG")"
    cp "$ROOT/hermes-control.toml" "$CONFIG"
    chmod 600 "$CONFIG"
fi

printf 'Installed hermes-control. Config: %s\nRun: %s/venv/bin/hermes-control --config %s\n' \
    "$CONFIG" "$APP_HOME" "$CONFIG"
