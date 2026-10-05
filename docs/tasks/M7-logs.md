# M7 — Logs Complete (Phase 5)

**Goal:** Full log viewer per brief §8–9. **Depends on:** M5. Parallel with M6.

### [x] M7-T1 `TransportCommandLogSource`
- Wraps `DeviceTransport::stream(log_command)` (e.g. `tail -n 200 -F <hermes log>` over Termux SSH). One persistent process — never one ADB call per line.
- Auto-restart on drop with backoff; emits a synthetic "stream reconnected" marker line.
- **Implementation note:** `TransportCommandLogSource` wraps a persistent transport stream, retries stream-open failures and ended/error streams with exponential backoff capped at 8 seconds, and emits one reconnect marker after a previously established stream recovers. Backoff resets after a stream remains up for 30 seconds. Cancellation stops retries; dropping the output receiver cancels the active remote tail.
- **Tests:** in-process transport verifies raw line order across reconnect, marker sequence, exponential backoff/cap, cancellation, receiver-drop cleanup, and empty-command rejection.

### [x] M7-T2 Log line parsing
- Detect timestamp (ISO-8601, `HH:MM:SS`, Python logging default `YYYY-MM-DD HH:MM:SS,mmm`) and level (INFO/WARN/WARNING/ERROR/DEBUG/CRITICAL→ERROR); preserve `raw`.
- Multi-line tracebacks: lines without timestamp attach to previous entry (configurable).
- **Implementation note:** `parse_log_line` recognizes ISO-8601, clock-only, Python logging, and existing Android threadtime records; severity aliases map to the shared levels. `LogLineParser` optionally joins timestamp-free continuation lines onto the preceding record while keeping the original text in `raw`; the transport-backed source uses this parser.
- **Tests:** timestamp/severity tables, raw-text preservation, multiline traceback attachment, and attachment disabled.

### [x] M7-T3 Source picker
- Sources: Hermes gateway log, Hermes tool-calls log, supervisor log (`~/.hacc/supervisor.log`), Android logcat, (later) API WebSocket. Same UI for all.
- Hermes files listed in `HermesConfig.log_files` are tailed from the Termux side via the proot host path (`installed-rootfs/debian/root/.hermes/logs/…`) — no proot overhead, survives gateway restarts and log rotation (`tail -F`).
- **Implementation note:** The Logs view selects Logcat, Hermes gateway, Hermes tool calls, or supervisor; active device buffers stay isolated and are cleared when a different source starts. Hermes log paths are editable in Settings and tailed via Termux SSH using the configured proot root with the old `installed-rootfs` fallback.
- **Tests:** source selection/start arguments, source-scoped buffer reset, shell-safe path construction for Termux/proot, and backward-compatible log path defaults.

### [x] M7-T4 Pause
- Pause rendering while continuing to buffer; badge "N new lines"; Resume jumps to latest.
- **Implementation note:** Pause freezes the displayed snapshot while the per-device ring buffer continues receiving batches. Resume reveals the buffered tail and restores auto-scroll; the new-line badge uses monotonic line sequence numbers.
- **Tests:** incoming batches update the count without changing visible rows; Resume renders the latest line and resumes auto-scroll.

### [x] M7-T5 Search
- Text and regex (with invalid-regex feedback), case toggle, match highlight, next/prev, debounced; runs in a Web Worker if buffer > 10k lines.
- **Implementation note:** Search stays synchronous below 10,001 visible lines and dispatches larger snapshots to a Web Worker; request signatures reject stale results. Search filters the displayed snapshot without mutating the ring buffer, highlights matches, reports invalid regex, and supports previous/next navigation.
- **Tests:** text/regex matching, case sensitivity, highlighting, invalid patterns, filtering without dropping buffered lines, and Worker search over 10,001 lines.

### [x] M7-T6 Level filter
- Toggle chips INFO/WARN/ERROR/DEBUG/Unknown with counts.
- **Implementation note:** Multi-select level chips compose with text/regex search and count the matching visible snapshot; “All” restores every level.
- **Tests:** all five levels, counts, single-level filtering, reset, and no mutation of the ring buffer.

### [x] M7-T7 Selection & copy
- Click / Shift-click / ⌘-click row selection; ⌘C copies raw lines; "Copy visible".
- **Implementation note:** Rows support single, Shift-range, and Ctrl/Meta toggled selection. Copy selected uses the raw lines in visible order; Copy visible copies the current filtered snapshot.
- **Tests:** single/range/toggled selection, selected raw copy, and visible copy.

### [x] M7-T8 Export
- Save dialog → `.log` (raw) or `.jsonl` (structured); export filtered or all; written by Rust (`export_logs`) to user-chosen path only.
- **Implementation note:** The Logs export dialog selects filtered-visible vs. all buffered lines and raw `.log` vs. structured `.jsonl`; Rust writes only to the native dialog's selected path.
- **Tests:** Rust format tests and UI IPC tests cover both formats and both scopes.

### [x] M7-T9 Auto-start
- If `auto_start_logs`, start Hermes source when device connects and Termux check passes.
- **Implementation note:** App-shell auto-start checks connected devices when the setting is enabled, requires a ready Termux checklist, and starts the Hermes gateway log without requiring the Logs view to be open. It skips devices with an existing or starting stream.
- **Tests:** ready, disabled, and not-ready startup conditions.

### [x] M7-T10 Performance
- Sustain 2 000 lines/s for 60 s without dropped frames > 100 ms; memory bounded by ring buffer.
- **Implementation note:** Chromium benchmark ran 120,000 synthetic IPC log lines over 60 seconds through the Logs view. The 20,000-line ring buffer retained sequences 100001–120000; 35 rows were mounted. Animation-frame gaps were 6.7ms p95 and 55ms maximum, with zero over 100ms.

## Exit check
- [ ] All brief §8 requirements demonstrable on real Hermes logs.
- **Remaining verification:** Run the source picker against live Hermes gateway/tool logs. The currently connected phone had an active Hermes session but no SSHD listener during the safe provisioning probe, so no live log command was sent.
