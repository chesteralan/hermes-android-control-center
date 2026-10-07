# Spike: Hermes command inventory (M0-S2)

Date: 2026-10-06 · Device: OPPO CPH2239, Android 11 / SDK 30, arm64-v8a · Termux 0.119.0-beta.3 · Hermes Agent v0.21.4

## Verified layout and commands

- Termux runs as `u0_a231`; `adb shell` runs as `uid=2000(shell)` and cannot list Termux's private home (`Permission denied`). Use the existing SSH bridge for Termux files and commands.
- Debian is installed through `proot-distro`; `proot-distro list` reports `debian`. Its root is `$PREFIX/var/lib/proot-distro/containers/debian/rootfs` (the older `installed-rootfs/debian` path is absent).
- Hermes CLI resolves to `/root/.local/bin/hermes`; the install directory is `/usr/local/lib/hermes-agent`. `hermes --version` reports v0.21.4, Python 3.13.5 for the Hermes install, and upstream commit `67f7e1d6`. The distro's `python --version` is Python 3.14.6, so use the Hermes venv interpreter for Hermes runtime version checks.
- Hermes data lives under `/root/.hermes`. `HERMES_HOME` was unset in a login shell, but the files are present under that home path.
- `hermes gateway --help` lists `run`, `start`, `stop`, `restart`, `status`, `install`, `uninstall`, `list`, and `setup`. Help says `gateway run` runs in the foreground and is recommended for Termux; `gateway install` installs a user/system service. The app's ADR-015 supervisor should run `hermes gateway run` in the foreground and manage its lifetime itself.
- `hermes gateway status` reported **not running** and a stale `gateway_state.json` claiming `running` while its recorded PID was gone. Treat the status file as advisory; verify the process and heartbeat.
- Current log directory contains `gateway.log`, `agent.log`, `errors.log`, `gateway-exit-diag.log`, `gateway-shutdown-diag.log`, `gateway_faulthandler.log`, `gui.log`, and `hacc-session-api.log`. `tool_calls.log` is absent on this install; configure tool-call source to the actual desired path rather than assuming it exists.
- A read-only `proot-distro login debian -- true` took 1.384 s in this run (a prior 2026-10-02 probe measured about 4.4 s). Cost varies with device load.
- A uniquely tagged inert `tail -f /dev/null` launched with `nohup setsid proot-distro login debian -- ...` remained visible after its SSH session closed. The test process was killed immediately after verification.
- A 200-line excerpt of `gateway.log` is in [gateway_log_redacted.txt](../../src-tauri/tests/fixtures/hermes/gateway_log_redacted.txt); timestamps and levels are retained, and every message is redacted.

## Still unverified

- `RUN_COMMAND` from ADB shell was denied with `Requires permission com.termux.permission.RUN_COMMAND` after a temporary, restored `allow-external-apps=true` probe; Termux also lacks shared-storage read/write permission.
- Whether `hermes gateway stop`/`install` behave under this device's proot environment and whether any service-manager operation is safe; no gateway stop/start/install commands were run during this read-only audit.
- SIGUSR1 drain and SIGTERM handling while a gateway is actually running. Current status is stopped, so no signal was sent.
- Hermes setup/configure prompts and their effects. Do not run setup against the user's configured profile without a disposable test home.

Prior observations and process details: [termux-access.md](termux-access.md). Provisioning/install observations: [provisioning.md](provisioning.md).
