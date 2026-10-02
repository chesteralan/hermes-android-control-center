# Brainstorm

## 1. Problem statement

Today Hermes Agent on the phone is managed by mirroring the screen (scrcpy) and typing into Termux. That is slow, fragile, and gives no structured view of status or logs. We want a desktop control panel that talks to the phone directly.

**Users:** a single developer/operator managing one (later several) Android phones on the same LAN.

**Success looks like:** open the app → device auto-connects → see Hermes is running → tail logs → restart Hermes with one click → run an ad-hoc command. No scrcpy.

## 2. Critical risk: the Termux access boundary

> This is the most important finding of the brainstorm and must be resolved in Milestone 0.

`adb shell` runs as the Android `shell` user (uid 2000). Termux runs as its own app uid (e.g. `u0_a123`) with a private data dir `/data/data/com.termux/files`. On a non-rooted phone:

- `adb shell` **cannot read** Termux's home, binaries (`$PREFIX/bin`), or log files.
- `run-as com.termux` does **not** work (release Termux is not debuggable).
- Therefore `adb shell "hermes status"` will **not** work out of the box.

The brief's "Execute commands inside Termux via `adb shell`" needs a bridge. Options:

| # | Bridge | How | Pros | Cons |
|---|---|---|---|---|
| A | **SSH over adb forward** | `sshd` in Termux on 127.0.0.1:8022 → `adb forward tcp:L tcp:8022` → Rust SSH client | Real Termux shell, stdout/stderr/exit code, streaming, mature | Needs `openssh` + key setup in Termux; extra crate (`russh`) |
| B | **Termux `RUN_COMMAND` intent** | `adb shell am startservice ... com.termux.RUN_COMMAND` | No extra server | Needs `allow-external-apps=true`; shell uid may lack `com.termux.permission.RUN_COMMAND`; no direct stdout capture; fire-and-forget |
| C | **Hermes Control API** (Phase 6) | Small HTTP/WS service in Termux on 127.0.0.1 → `adb forward` | Purpose-built, structured JSON, WebSocket logs | Must be written and deployed; needs a bootstrap path |
| D | **Shared storage** | Hermes writes logs to `/sdcard/...`; adb reads them | Simple for logs | Requires `termux-setup-storage`; read-only; no control |
| E | **logcat** | Hermes pipes logs to Android log | `adb logcat` streams natively | Requires Hermes-side change; tag filtering only |

**Recommendation**

- Phase 2 terminal = plain `adb shell` (Android shell). Matches brief, useful immediately, honest about what it is.
- Introduce a `DeviceTransport` with two Termux-capable implementations:
  1. `TermuxSshTransport` (Option A) — first Termux bridge, Milestone 4.
  2. `ApiTransport` (Option C) — Milestone 8; replaces SSH for structured status/logs.
- Keep Option B as a spike only (verify in M0; adopt for "bootstrap start sshd" if it works).
- Critical insight: **every Termux-side listener binds to `127.0.0.1` and is reached through `adb forward`.** Nothing is exposed on the LAN, so no auth infrastructure is required for local-only access (brief §17), while still allowing a token as defense in depth.

## 3. Other risks and unknowns

| Risk | Impact | Mitigation |
|---|---|---|
| macOS GUI apps don't inherit shell `PATH` | `adb` "not found" when launched from Finder | Probe absolute paths (`/opt/homebrew/bin`, `/usr/local/bin`, `~/Library/Android/sdk/platform-tools`, `$ANDROID_HOME`), persist chosen path |
| Wireless debugging port changes on every toggle/reboot | Reconnect to stale port fails | Use `adb mdns services` to rediscover `_adb-tls-connect._tcp`; fall back to user prompt |
| Android kills Termux in background (phantom process killer, Doze) | Hermes dies silently | Surface "Termux unavailable" state; doc `termux-wake-lock`, battery optimization, phantom-process settings |
| Hermes CLI/commands unknown/variable | Hard-coded commands break | All commands in `HermesConfig`; status parsing tolerant; show `Unknown` rather than guess |
| Hermes installed inside proot-distro | Commands/Python/install checks run in the wrong environment | `HermesEnvironment` wrapper (Termux / proot-distro / custom) + detection across Termux and all distros (ADR-013) |
| Log floods overwhelm IPC/UI | UI freezes | Batch lines in Rust (≤50 ms / ≤500 lines), virtualized list, ring buffer cap |
| Long-running commands hang | Stuck UI / zombie processes | Every spawn is cancellable, `kill_on_drop`, timeouts on non-streaming calls |
| `adb` server restarts / version mismatch | Spurious disconnects | Detect "daemon not running"/"server version" output; `adb start-server` once |
| Multiple devices with same model | Wrong target | Always address by serial (`-s`); never default when >1 device; name the phone in every confirm dialog |
| Wireless serial changes with port | Lost tab/history/host key | Stable `device_id` from `ro.serialno` (ADR-012) |
| Many phones streaming logs | CPU/memory growth | Per-device buffers, max concurrent streams setting |
| Quoting through `adb shell` | Injection / broken commands | Host side: argv only, never `sh -c`. Device side: configured commands are templates with shell-escaped substitutions |

## 4. Ideas considered (and status)

| Idea | Decision |
|---|---|
| Polling `adb devices` every second | **Rejected** → use `adb track-devices` persistent stream; poll only as fallback |
| Tauri events for log streaming | **Partially** → use `tauri::ipc::Channel` per stream (ordered, scoped, faster); global events for device state changes |
| `adb` Rust protocol crate (talk to adb server on :5037 directly) | **Deferred** → shelling out to `adb` binary is simpler and the brief requires it; transport trait keeps the option open |
| Bundling `adb` | **Rejected for v1** (brief §11) |
| Generate TS types from Rust | **Accepted** → `ts-rs` (see ADR-006) |
| Screen mirroring | **Out of scope** |
| Pairing UI (`adb pair ip:port code`) | **Accepted** — small effort, big UX win for Wireless ADB |
| mDNS discovery (`adb mdns services`) | **Accepted** as "Discover" button + reconnect helper |
| Menu-bar tray icon with Hermes status | **Post-1.0 idea** |
| Notifications when Hermes crashes | **Post-1.0 idea** (`tauri-plugin-notification`) |
| Auto-updater | **Accepted for production** (`tauri-plugin-updater`) |
| Multi-device tabs | **Accepted** → M9 (ADR-012); backend + stores keyed by device from M2 |

## 5. Superpowers (differentiators worth building)

1. **One-click setup** – from a factory-fresh phone, the app installs Termux, bootstraps SSH, installs proot-distro and Hermes from an editable recipe, and enables autostart (M11, ADR-014). For existing setups it shows a checklist with copy-paste fix commands.
2. **Self-healing connection** – `track-devices` + mDNS rediscovery + backoff means the port-change problem disappears for the user.
3. **Unified stream model** – terminal output, Hermes logs, and logcat all flow through one `StreamEvent` type and one virtualized viewer component.
4. **Command palette (⌘K)** – run saved commands / Hermes actions without leaving the keyboard.
5. **Saved command snippets** – per-device favorites with confirmation flags for destructive ones.
6. **Health timeline** – record state transitions (connected/disconnected, Hermes up/down) to show "Hermes restarted 3× in the last hour".
7. **Diagnostics bundle** – export app logs + device info + last N Hermes log lines (secrets redacted) for bug reports.

Items 1–3 are in the roadmap; 4–7 are stretch goals in M10 or post-1.0.

8. **Fleet view** – one tab per phone plus an Overview grid showing every phone's Hermes status at a glance (M9).

## 6. Non-goals (v1)

- Screen mirroring, input injection, file manager.
- Windows/Linux builds **for v1.0** — planned for v1.1 (M12, ADR-016); the core is kept portable from M0.
- Root-only features.
- Remote (Internet) access to the phone.
- Mac App Store distribution (sandbox forbids spawning `adb`).

## 7. Open questions

1. ~~What exact commands start/stop Hermes?~~ **Answered:** default target is Debian proot + official installer (`install.sh`); gateway runs as `hermes gateway run` under the app's supervisor (ADR-015); `hermes gateway status`, `hermes status`, `hermes doctor`, `hermes --version`, `hermes update`.
2. ~~Where does Hermes write logs?~~ **Answered:** `~/.hermes/logs/gateway.log` (+ `tool_calls.log`, `gateway_faulthandler.log`); state in `gateway_state.json`. Exact paths re-checked in M0-S2.
3. Does Hermes expose any status endpoint already? If so, ApiTransport can wrap it.
4. Is root ever available on target phones? (Assume no.)
5. Does `am startservice ... RUN_COMMAND` work from the `shell` uid on Android 14/15? (M0 spike.)
