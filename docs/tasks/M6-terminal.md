# M6 — Terminal Complete (Phase 4)

**Goal:** Productive terminal experience. **Depends on:** M5. Parallel with M7.

### [x] M6-T1 Command history
- Per `device_id` + transport, persisted (last 500, dedupe consecutive) in store; ↑/↓ navigation, Ctrl+R reverse search.
- Setting to disable history persistence.
- **Implementation note:** History is stored by stable device key and transport in local storage. Disabling “Save history” removes saved command histories while retaining in-memory navigation until app exit.
- **Tests:** ↑/↓ navigation and draft restoration, repeated Ctrl+R search, reload, device/transport separation, consecutive dedupe, 500-entry cap, and persistence opt-out.

### [x] M6-T2 Multiple sessions (tabs)
- Terminal sessions belong to one device; with M9 they nest inside that phone's device tab.
- Tabs with independent scrollback and running command; ⌘T new, ⌘W close (cancels running stream with confirm).
- **Implementation note:** Terminal sessions are independently stored per stable device key; Mod+T creates a tab, Mod+W closes it, and closing a tab with a running command asks for confirmation and cancels that stream.
- **Tests:** independent tab output, keyboard shortcuts, close confirmation, and stream cancellation.

### [x] M6-T3 ANSI rendering
- Parse SGR color codes (e.g. `anser` or small in-house parser); strip other escape codes safely; no HTML injection (render as React text nodes).
- **Implementation note:** Supports standard/bright and 256/RGB foreground/background colors plus common text styles; unknown, non-SGR, and incomplete control sequences are stripped.
- **Tests:** colors, malformed sequences, and `<script>` output rendered as text with no script element.

### [x] M6-T4 Interrupt & timeouts
- Ctrl+C cancels running command; optional per-command timeout setting; show "cancelled" footer.
- **Implementation note:** Ctrl+C delegates to the active stream cancellation path and displays the cancelled footer. The Terminal view offers an optional timeout from 0 (off) through 3,600 seconds; timeout expiry cancels the stream and clears its timer on completion or cancellation.

### [x] M6-T5 Virtualized scrollback
- 10 000 lines per session, smooth at 1 000 lines/s.
- **Implementation note:** The store trims scrollback to 10,000 lines across command blocks, including an oversized single block, by dropping only the oldest excess lines. Terminal output virtualizes flattened rows after 500 lines; tests verify the cap and that a 10,000-line transcript mounts only a viewport slice. Streamed line updates are coalesced into 16ms batches; a deterministic 1,000-lines/second test preserves all lines with 63 store updates. A Chromium browser run of the actual TerminalView streamed 1,008 lines in 63 batches with 12.4ms p95 and 30.5ms maximum animation-frame gaps; no frame exceeded 100ms.

### [x] M6-T6 Saved snippets
- Name, command, transport, `requires_confirmation` flag; run from the terminal Snippets strip; confirmation dialog shows the exact command.
- **Implementation note:** Saved snippets persist locally and appear in the terminal's Snippets strip. Each runs with its saved transport; snippets marked for confirmation show the exact command before execution.
- **Tests:** save/persist a snippet, cancel confirmation without running, and run the confirmed command with its saved transport.

### [x] M6-T7 Copy/export
- Copy block output, copy all, export session to `.txt` via save dialog.
- **Implementation note:** Copy actions use the clipboard; `.txt` export uses a native Tauri save dialog and Rust writes only to the selected path. The frontend receives only the result/cancel status.
- **Tests:** block/all copy payloads and export text passed to the native-save IPC command.

### [x] M6-T8 Interactive PTY sessions (required by M11)
- SSH transport only: request a PTY (`russh` pty-req), forward keystrokes and terminal resize, render with a terminal emulator component (`xterm.js` via `@xterm/xterm`).
- Used for interactive setup wizards (e.g. Hermes configuration) and full-screen tools; PTY output is never persisted to history or logged.
- **Implementation note:** Interactive sessions use a separate SSH PTY channel and xterm.js view; terminal input/output bypasses command history and scrollback. Resize dimensions are clamped, and PTYs close on user close, device release, or window destruction.
- **Tests:** in-process SSH server verifies PTY request, shell startup, input/output, resize forwarding, and cancellation; component test verifies raw output/input routing and that interactive input is not persisted.

## Exit check
- [x] Long-running stream (`ping -c 100`) doesn't block UI; history survives restart.
