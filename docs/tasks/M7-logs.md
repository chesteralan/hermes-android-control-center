# M7 — Logs Complete (Phase 5)

**Goal:** Full log viewer per brief §8–9. **Depends on:** M5. Parallel with M6.

### [ ] M7-T1 `TransportCommandLogSource`
- Wraps `DeviceTransport::stream(log_command)` (e.g. `tail -n 200 -F <hermes log>` over Termux SSH). One persistent process — never one ADB call per line.
- Auto-restart on drop with backoff; emits a synthetic "stream reconnected" marker line.

### [ ] M7-T2 Log line parsing
- Detect timestamp (ISO-8601, `HH:MM:SS`, Python logging default `YYYY-MM-DD HH:MM:SS,mmm`) and level (INFO/WARN/WARNING/ERROR/DEBUG/CRITICAL→ERROR); preserve `raw`.
- Multi-line tracebacks: lines without timestamp attach to previous entry (configurable).
- **Tests:** fixtures from M0-S2 Hermes logs, Python tracebacks, unstructured text.

### [ ] M7-T3 Source picker
- Sources: Hermes gateway log, Hermes tool-calls log, supervisor log (`~/.hacc/supervisor.log`), Android logcat, (later) API WebSocket. Same UI for all.
- Hermes files listed in `HermesConfig.log_files` are tailed from the Termux side via the proot host path (`installed-rootfs/debian/root/.hermes/logs/…`) — no proot overhead, survives gateway restarts and log rotation (`tail -F`).

### [ ] M7-T4 Pause
- Pause rendering while continuing to buffer; badge "N new lines"; Resume jumps to latest.

### [ ] M7-T5 Search
- Text and regex (with invalid-regex feedback), case toggle, match highlight, next/prev, debounced; runs in a Web Worker if buffer > 10k lines.

### [ ] M7-T6 Level filter
- Toggle chips INFO/WARN/ERROR/DEBUG/Unknown with counts.

### [ ] M7-T7 Selection & copy
- Click / Shift-click / ⌘-click row selection; ⌘C copies raw lines; "Copy visible".

### [ ] M7-T8 Export
- Save dialog → `.log` (raw) or `.jsonl` (structured); export filtered or all; written by Rust (`export_logs`) to user-chosen path only.

### [ ] M7-T9 Auto-start
- If `auto_start_logs`, start Hermes source when device connects and Termux check passes.

### [ ] M7-T10 Performance
- Sustain 2 000 lines/s for 60 s without dropped frames > 100 ms; memory bounded by ring buffer.

## Exit check
- [ ] All brief §8 requirements demonstrable on real Hermes logs.
