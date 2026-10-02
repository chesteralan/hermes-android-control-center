# M3 — Basic Terminal + Live Logs (First Working Milestone)

**Goal:** Complete brief §24 items 8–11. **Depends on:** M2.

## Backend

### [x] M3-T1 `DeviceTransport` trait + `AdbShellTransport`
- Trait per ARCHITECTURE §4.2. `AdbShellTransport::execute` → `adb -s S shell <cmd>`; exit code is propagated by modern adb (shell v2) — verify; fallback: append `; echo "@@exit:$?"` and parse.
- `stream` → spawn, map stdout/stderr lines to `StreamEvent`, final `Exit { code, duration_ms }`.
- **Tests:** FakeRunner exec + stream; exit code parsing fallback.

### [x] M3-T2 `StreamRegistry`
- Each stream records its owning device; `cancel_device(device_id)` cancels only that phone's streams.
- `register() -> (StreamId, CancellationToken)`, `cancel(id)`, `cancel_all()`; cancel on window close.
- **Tests:** cancel stops stream and kills child (FakeRunner observes drop).

### [x] M3-T3 Terminal commands
- `execute_command(serial, command)` (default 60 s timeout) and `stream_command(serial, command, on_event: Channel<StreamEvent>) -> StreamId`.
- Reject empty command; never execute without explicit user submission (no auto-run on load).
- Commands logged at Debug only, output never logged.

### [x] M3-T4 `LogSource` trait + `LogcatSource`
- `LogcatSource` runs `adb -s S logcat -v threadtime <filter>` (filter from settings; default `*:I`).
- Level detection: logcat letter (V/D/I/W/E/F) → `LogLevel`; generic regex for `INFO|WARN(ING)?|ERROR|DEBUG` in other sources; `raw` always preserved.
- **Tests:** threadtime fixture parsing; unstructured line keeps `level: None`.

### [x] M3-T5 Log batcher + `start_log_stream`
- Batch to `Channel<Vec<LogLine>>` every 50 ms or 500 lines; monotonic `seq`.
- `start_log_stream(serial, source, on_batch) -> StreamId`; reuse `cancel_stream`.
- **Tests:** batch boundaries by size and time (tokio paused clock).

## Frontend

### [x] M3-T6 Terminal view (basic)
- Header label: **"Android shell (adb shell) — runs as shell user, not Termux"**.
- Prompt input; Enter submits; output blocks per command: `$ cmd`, stdout (default color), stderr (red), footer `exit 0 · 132 ms`.
- Running state with Cancel button; UI stays responsive.
- **Tests:** submit renders streamed events in order; stderr styled; cancel calls `cancel_stream`.

### [x] M3-T7 Log viewer (basic)
- Virtualized list (`@tanstack/react-virtual`), columns: time, level badge, message.
- Start/Stop stream, auto-scroll (disables when user scrolls up, "Jump to latest" button), Clear.
- Ring buffer 50 000 lines (per device; buffers stored under the device key).
- **Tests:** batches append; clear empties; auto-scroll toggle.

### [x] M3-T8 Dashboard "Recent Logs"
- Last 20 lines of active stream.

## Release v0.1.0

### [x] M3-T9 Root README v0.1
- Prerequisites, install, run (`npm install`, `npm run tauri dev`), link to `docs/guides/ANDROID_SETUP.md`.

### [ ] M3-T10 Manual QA (first milestone checklist)
1. [ ] Launch app
2. [ ] adb detected
3. [ ] devices listed
4. [ ] device displayed
5. [ ] device selected
6. [ ] connect works
7. [ ] device info shown
8. [ ] `adb shell` command executes
9. [ ] stdout/stderr displayed
10. [ ] terminal usable
11. [ ] live logcat streams

- [ ] Tag `v0.1.0`, attach unsigned `.app` from CI.
