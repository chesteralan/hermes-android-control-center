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

### [x] M3-T10 Manual QA (first milestone checklist)
1. [x] Launch app
2. [x] adb detected
3. [x] devices listed
4. [x] device displayed
5. [x] device selected
6. [x] connect works
7. [x] device info shown
8. [x] `adb shell` command executes
9. [x] stdout/stderr displayed
10. [x] terminal usable
11. [x] live logcat streams

- [ ] Tag `v0.1.0`, attach unsigned `.app` from CI.

## Live QA record (2026-10-07)
- Tested the isolated workspace-built `HACC QA` macOS app (`com.hermes.controlcenter.qa`) against CPH2239 on Android 11 over Wireless ADB. The installed production app was not modified. Device listing, selection, Connect, and identity deduplication also have live evidence in [M2's QA record](M2-adb-core.md).
- Launch showed the selected phone as Connected with live model, Android version, battery, storage, and Termux metadata.
- In Android shell mode, Enter submitted `printf "M3_QA_STDOUT\n"; printf "M3_QA_STDERR\n" >&2; exit 7`. The UI displayed both markers, styled stderr red, and reported `exit 7` with a 185 ms duration.
- Submitted `logcat -v brief -s HACC_M3_QA:I` in Terminal. A phone-side `log -p i -t HACC_M3_QA M3_TERMINAL_STREAM` marker appeared while the command was running. Cancel changed the block to `cancelled` and restored the terminal controls.
- In Logs, selected Android logcat and searched for `HACC_M3_QA`. Start enabled Stop; a fresh `M3_LOG_VIEWER_LIVE` marker appeared with its timestamp and INFO level. Stop returned to Start and the buffer stayed at 1,010 lines after another phone-side marker. Clear reset the buffer to zero and disabled copy/export.
- Scope: this verifies the macOS basic terminal/logcat checklist, not Windows/Linux runtime, high-volume performance, or every advanced log control. The log toolbar overflowed horizontally at a 1200 px window width; this was resolved on 2026-10-08 by wrapping controls and level filters below the summary.
- Toolbar follow-up: browser checks reproduced the original overflow and verified all controls remain inside the panel at 800, 900, 1200, and 1600 px window widths, including populated counters and a long Resume label. Search and Resume worked without browser errors; all 15 Logs tests, typecheck, and component lint passed.
- No historical `v0.1.0` tag was created. Public releases still require the current security, signing, and install/update gates.
