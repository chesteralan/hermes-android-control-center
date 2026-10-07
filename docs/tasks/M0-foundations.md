# M0 — Foundations & Spikes

**Goal:** Resolve unknowns and scaffold tooling. **Depends on:** nothing.

## Spikes (time-boxed, output = notes in `docs/spikes/`)

### [x] M0-S1 Termux access from ADB
- Confirm `adb shell ls /data/data/com.termux/files/home` fails (permission denied) on target phone.
- Try `RUN_COMMAND` intent from `adb shell`:
  `am startservice --user 0 -n com.termux/com.termux.app.RunCommandService -a com.termux.RUN_COMMAND --es com.termux.RUN_COMMAND_PATH /data/data/com.termux/files/usr/bin/sshd --ez com.termux.RUN_COMMAND_BACKGROUND true`
  with `allow-external-apps=true` in `~/.termux/termux.properties`. Record result per Android version.
- Install `openssh` in Termux, run `sshd -o ListenAddress=127.0.0.1 -p 8022`, then `adb forward tcp:8022 tcp:8022` and `ssh -p 8022 localhost` from Mac.
- **Output:** `docs/spikes/termux-access.md` — confirm/adjust ADR-004.
- **Implementation note:** `adb shell` cannot access Termux private files. A user-approved `allow-external-apps=true` RUN_COMMAND probe was denied because the ADB-shell sender lacks `com.termux.permission.RUN_COMMAND`; the setting and mode-0600 properties file were restored. The app's pinned SSH key connected through ADB forwarding as Termux uid `u0_a231`, confirming SSH as the supported bridge. See [termux-access.md](../spikes/termux-access.md).

### [x] M0-S2 Hermes command inventory
- On the target install, record: how Hermes is installed (native Termux or inside `proot-distro` — which distro), how it is started (foreground/background, `nohup`, `tmux`, `termux-services`/`sv`), gateway start, stop method, status method, log location/format, Python version command.
- proot-distro specifics:
  - Output of `proot-distro list` and contents of `$PREFIX/var/lib/proot-distro/installed-rootfs/`.
  - What Hermes looks like from the Termux side: `pgrep -af hermes`, `ps -o pid,etimes,args`, `cat /proc/<pid>/cmdline` (does the guest argv show, or the proot loader?).
  - Same from `adb shell ps -A -o PID,ETIME,ARGS` (visible to uid `shell`?).
  - Does `nohup setsid proot-distro login <d> -- … &` survive closing the SSH session?
  - Cost of `proot-distro login <d> -- true` (latency per call).
- Capture 200 lines of real log output (redacted) as a fixture.
- **Output:** `docs/spikes/hermes-commands.md` + `src-tauri/tests/fixtures/hermes/*.txt`.
- **Implementation note:** The command/layout/status inventory is in [hermes-commands.md](../spikes/hermes-commands.md), with a 200-line message-redacted gateway fixture. A tagged inert proot process survived SSH disconnect and was cleaned up; `proot-distro login debian -- true` took 1.384 s. CLI help documents foreground `hermes gateway run` and its status/stop commands. The gateway was already stopped with stale state, so live gateway start/stop and signal behavior remain untested.

### [~] M0-S4 Provisioning feasibility (fresh or reset phone)
- `adb install` of Termux APK (F-Droid and GitHub builds); note OEM prompts.
- Locate download metadata + SHA-256 source for each (F-Droid index, GitHub release assets/checksums).
- `pm grant` storage/notification permissions for Termux; Termux can read/write `/sdcard/Download/hacc/`.
- `am start` Termux, detect bootstrap completion; `input text` + `keyevent 66` reliably runs a command in Termux.
- `dumpsys deviceidle whitelist`, phantom-process commands on Android 12, 13, 14, 15.
- `am start` Termux:Boot activates boot scripts; Hermes survives reboot.
- Record the exact Hermes install + configure steps inside the chosen distro (becomes the default recipe).
- Debian + official installer: does `install.sh` prompt (needs PTY)? Which PATH/profile files does it edit? Does `hermes setup` / `hermes gateway setup` try to install a systemd service and how does it fail? Does `hermes gateway run` honor SIGUSR1 drain and SIGTERM in proot? Confirm `~/.hermes/gateway_state.json` + `~/.hermes/logs/*` paths.
- **Output:** `docs/spikes/provisioning.md`.
- **Progress:** Release metadata, GitHub APK checksum, read-only CPH2239 state, and the RUN_COMMAND permission denial are recorded in [provisioning.md](../spikes/provisioning.md). Fresh install, storage access, keystroke bootstrap, Termux:Boot, and reboot checks remain unverified; the spare/reset phone is not currently connected.

### [~] M0-S3 ADB output fixtures
- Capture raw output (redact serials/IPs) for: `adb version`, `adb devices -l` (none / USB / wireless / unauthorized / offline / multiple), `adb track-devices -l` (hex-length frames), `adb connect` (success / refused / already connected / timeout / failed to authenticate), `adb pair` (success / wrong code), `adb disconnect`, `adb mdns check`, `adb mdns services` (idle, and while the phone shows "Pair device with QR code" after scanning), `getprop`, `dumpsys battery`, `df -k /data`, `/proc/meminfo`, `/proc/cpuinfo`, `ip -f inet addr show wlan0`.
- **Output:** `src-tauri/tests/fixtures/adb/*.txt`.
- **Progress:** Captured ADB version, no-device and wireless-alias listings, host-local refused and live already-connected results, selected getprop fields, battery, `/data` df, meminfo, CPU summary, WLAN IP, and idle/live mDNS output with serial/IP redaction. Verified the sanitized `track_frames.bin` frame's `0074` prefix matches its 116-byte payload, with an explicit parser regression assertion. USB, unauthorized/offline, authentication/timeout/pairing failures, QR discovery, and alternate Android versions still need device-specific captures; see [fixtures README](../../src-tauri/tests/fixtures/README.md).

## Scaffolding

### [x] M0-T1 Create Tauri 2 + React + TS + Vite project
- `npm create tauri-app@latest` (React, TypeScript, npm) into repo root; preserve `docs/`.
- App identifier `com.hermes.controlcenter` (confirm), product name "Hermes Control Center".
- **DoD:** `npm install && npm run tauri dev` opens a window.

### [x] M0-T2 Tooling
- TS: `strict: true`, `noUncheckedIndexedAccess`, ESLint (typescript-eslint, react-hooks), Prettier.
- Rust: `rustfmt.toml`, clippy config, `rust-toolchain.toml` pinned stable.
- Scripts: `dev`, `build`, `tauri`, `lint`, `typecheck`, `test`, `test:rust`, `format`.
- **DoD:** all scripts run green on the scaffold.

### [x] M0-T3 Tailwind v4
- Install `tailwindcss` + `@tailwindcss/vite`; dark theme tokens via `@theme` (bg, surface, border, text, muted, accent, success, warning, danger); JetBrains Mono / system mono for data.
- **DoD:** a test page renders with dark tokens.

### [x] M0-T4 Test harnesses
- Vitest + jsdom + @testing-library/react + user-event; `src/test/setup.ts` mocking `@tauri-apps/api/core` `invoke` and `Channel`.
- Rust: `tests/` dir, fixtures loader helper.
- **DoD:** one passing sample test in each.

### [x] M0-T5 CI skeleton (GitHub Actions, 3-OS matrix)
- Matrix: `macos-latest`, `windows-latest`, `ubuntu-latest` (Linux deps: `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev`). macOS is the release gate until M12; others must stay green.
- Jobs: `frontend` (npm ci, lint, typecheck, test), `rust` (fmt, clippy -D warnings, test), `build` (tauri build, unsigned) on PRs.
- Cache npm and cargo.
- **DoD:** pipeline green on main.

### [x] M0-T6 Repo hygiene
- `.gitignore` (node_modules, dist, target, .DS_Store), `.editorconfig`, `LICENSE` (choose), PR template with DoD checklist, root `README.md` stub linking to `docs/`.
- **Implementation note:** Added the selected MIT license; `.gitignore`, `.editorconfig`, PR template, and README-to-docs link were already present.
