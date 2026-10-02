# M2 — ADB Core (Phase 2)

**Goal:** Device discovery, Wireless ADB connect/pair, device info, connection monitoring. **Depends on:** M1, M0-S3 fixtures.

## Backend

### [x] M2-T1 ADB locator (`adb/locate.rs`)
- Order: configured path → `which adb` on PATH → per-OS candidates from `platform::adb_candidates()`. macOS: `/opt/homebrew/bin/adb` → `/usr/local/bin/adb` → `$ANDROID_HOME/platform-tools/adb` → `$ANDROID_SDK_ROOT/...` → `~/Library/Android/sdk/platform-tools/adb`. Windows/Linux lists defined now, verified in M12-T1.
- Validate by running `adb version`; parse version + install path.
- Command `detect_adb() -> AdbInfo`; returns `AdbNotFound { searched }` listing every path tried.
- **Tests:** with fake filesystem/env probe; version parsing from fixture.

### [x] M2-T2 `AdbClient` argv builders (`adb/client.rs`)
- Pure builders returning `Vec<String>`: `devices_l`, `track_devices`, `connect(addr)`, `disconnect(addr|serial)`, `pair(addr, code)`, `mdns_services`, `shell(serial, cmd)`, `forward(serial, local, remote)`, `forward_remove`, `logcat(serial, args)`, `start_server`.
- Always prefix `-s <serial>` for device-scoped calls.
- Validate `ip:port` (IPv4/IPv6/hostname + port range) before use → `AppError::Config`.
- **Tests:** exact argv for every builder; invalid addresses rejected; serial with special chars passed as single arg.

### [x] M2-T3 Output parsers (`adb/parse.rs`)
- `parse_devices_l`, `parse_track_frame` (4-hex-length framing), `parse_connect_output` (→ Ok / `ConnectionRefused` / `Timeout` / `DeviceUnauthorized` / `WirelessDebuggingDisabled` heuristics), `parse_pair_output`, `parse_mdns_services`, `parse_getprop`, `parse_dumpsys_battery`, `parse_df_k`, `parse_meminfo`, `parse_cpuinfo`, `parse_ip_addr`.
- Note: `adb connect` may exit 0 on failure — parse text, not just exit code.
- **Tests:** every M0-S3 fixture, plus garbage/empty input never panics (add `proptest` for parsers).

### [x] M2-T4 Error mapping
- Map stderr patterns → `AppError`: `device offline`, `unauthorized`, `device '...' not found`, `Connection refused`, `cannot connect`, `failed to authenticate`, `daemon not running` (auto `start-server` once), `No such file` (adb missing).
- **Tests:** table-driven from fixtures.

### [x] M2-T5 Device model + `DeviceManager`
- `AndroidDevice`, `DeviceState`, `DeviceInfo` per ARCHITECTURE §3; `HashMap<serial, DeviceEntry>` in state (multi-device ready).
- `get_device_info(serial)` uses single sectioned shell call (ARCHITECTURE §6), 10 s timeout.
- **Tests:** combined sectioned fixture → full `DeviceInfo`; missing sections → `None` fields.

### [x] M2-T5a Stable `device_id`
- On connect, resolve `device_id` via `getprop ro.serialno` → `ro.boot.serialno` → `settings get secure android_id`; store on `AndroidDevice`.
- Wireless serial (`ip:port`) may change; `device_id` must not. Used later for profiles, tabs, history and host keys (M9).
- **Tests:** each fallback; empty/`unknown` values skipped.

### [x] M2-T5b Termux package detection (plain ADB, no SSH needed)
- `adb -s S shell pm list packages -i --show-versioncode com.termux` + `dumpsys package com.termux | grep -E 'versionName|firstInstallTime'`.
- `TermuxPackageInfo { installed, version_name, version_code, installer }` added to `DeviceInfo.termux`.
- Warn when installer is Google Play (outdated build) — recommend F-Droid/GitHub.
- Also report companion apps if present (`com.termux.api`, `com.termux.boot`) — informational only.
- **Tests:** installed (F-Droid / Play / unknown installer), not installed.

### [x] M2-T6 Commands
- `list_devices`, `connect_device(address)`, `disconnect_device(serial)`, `pair_device(address, code)`, `discover_devices`, `get_device_info(serial)`.
- Pairing code never logged.
- **Tests:** commands with `FakeRunner`.

### [x] M2-T6a QR-code pairing (`adb/qr_pair.rs`)
Mirrors Android's "Pair device with QR code" flow:
1. `start_qr_pairing(on_event: Channel<QrPairEvent>) -> PairingSessionId` generates a random service name (`hacc-<8 random chars>`) and password (CSPRNG, ≥ 10 chars) and returns QR payload `WIFI:T:ADB;S:<name>;P:<password>;;` rendered to SVG in Rust (`qrcode` crate).
2. Watches `adb mdns services` (every 1 s, max 2 min, time-limited) for `_adb-tls-pairing._tcp` with instance name `<name>` → gets `ip:port`.
3. Runs `adb pair <ip:port> <password>`; on success, finds the phone's `_adb-tls-connect._tcp` service on the same IP and runs `adb connect` (skip if adb already auto-connected it).
4. Emits `QrPairEvent`: `Waiting` → `Found { address }` → `Paired` → `Connected { device }` | `Failed(AppError)` | `Expired`.
- `cancel_qr_pairing(id)`; new password per attempt; password never logged or persisted.
- Precheck with `adb mdns check`; if mDNS unavailable → error suggesting code pairing.
- **Tests:** payload format/escaping; FakeRunner scripted mdns outputs (not found → found → paired → connected); expiry with paused clock; wrong-password output → error.

### [x] M2-T7 Device tracker (`adb/tracker.rs`)
- Background task running `track-devices -l`; diffs snapshots; emits `device://changed`.
- On child exit: restart with backoff; after 3 failures fall back to polling `devices -l` every 5 s.
- **Tests:** scripted stream frames → expected emitted snapshots.

### [x] M2-T8 Reconnect with backoff
- Implemented scope: every wireless device seen in `device` state is supervised (including mDNS auto-connected ones), unless the user disconnected it.
- Pure `Backoff { schedule, attempt }` + one independent `ReconnectSupervisor` per wireless device that was connected by the user (several may run at once).
- On loss: emit `device://reconnect` per attempt; after attempt ≥2 query mDNS for same IP with new port.
- Gives up after schedule exhausted; `retry_connection(serial)` command resets.
- User-initiated disconnect does **not** trigger reconnect.
- **Tests:** fake clock; schedule `1,2,5,10,30…`; give-up; manual disconnect suppression; mDNS port update; two devices reconnecting independently.

### [x] M2-T9 Auto-connect on launch
- Connect every remembered address with `auto_connect` at startup (list, not a single value — becomes per-device profiles in M9); remember address on successful manual connect.

## Frontend

### [x] M2-T10 `deviceStore`
- Devices map keyed by `device_id`, active device id, reconnect status per device; subscribes to `device://changed` / `device://reconnect`.
- Auto-select when exactly one device; never auto-select among many.
- Per-device sub-state (info, errors) lives under the device key so tabs (M9) need no store rewrite.

### [x] M2-T11 Connect panel
- Inputs: IP, Port (validated), Connect / Disconnect; "Pair new device" dialog (IP, pairing port, 6-digit code); "Discover" (mDNS results list → click to connect).
- Hint text distinguishing the **pairing port** from the **connect port**.
- **Tests:** validation, loading state, error panel on `ConnectionRefused`.

### [x] M2-T11a QR pairing UI
- "Pair new device" dialog has two tabs: **QR code** (default) and **Pairing code**.
- QR tab: shows SVG as an `<img>` data URL (never injected HTML), instruction "Phone → Wireless debugging → Pair device with QR code", live status from `QrPairEvent`, countdown, Regenerate, Cancel; on `Connected` closes and selects the phone.
- On mDNS failure or expiry: offer "Use pairing code instead" switching tabs with IP prefilled if known.
- **Tests:** each event state renders; cancel calls `cancel_qr_pairing`; fallback switch.

### [x] M2-T11b No-devices empty state
- When no phones are listed, Dashboard/Device/Hermes/Terminal/Logs show a "No phone connected" panel with actions: **Pair with QR code**, **Pair with code**, **Connect by IP**, **Discover**; plus a link to setup guide steps.
- If ADB isn't found, the ADB banner (M2-T14) takes priority.
- **Tests:** empty state renders all actions; each opens the right dialog/tab.

### [x] M2-T12 Device list + selector
- Header dropdown + Device page list: serial, model, state dot, wireless badge.
- States: connected, offline, unauthorized (with "Accept the prompt on your phone" hint), connecting, reconnecting (attempt n, next in Xs), gave up (Retry button).
- **Tests:** render each state.

### [x] M2-T13 Device info card
- Fields per brief §4; storage/memory humanized; "Unknown" for `None`; Refresh button; auto-refresh every 30 s while visible.
- **Tests:** full info, partial info, error state.

### [x] M2-T14 Settings → ADB detect
- Detect button calls `detect_adb`, shows path/version or `AdbNotFound` with searched paths + install hint.
- First-run banner on Dashboard when adb not found.

### [x] M2-T15 Dashboard v1
- Device card (status, model, Android, IP) + placeholders for Hermes and Recent Logs.

## Exit check
- [ ] Replace `adb devices`, `adb pair`, `adb connect`, `adb disconnect` with the UI on a real phone.
- [ ] Pair a fresh phone via QR code and via pairing code, starting from the no-devices screen.
- [ ] Toggle wireless debugging off → Disconnected → reconnect attempts → Retry works.
- [ ] CI green, tag `v0.1.0-alpha.2`.
