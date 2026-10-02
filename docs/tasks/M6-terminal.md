# M6 — Terminal Complete (Phase 4)

**Goal:** Productive terminal experience. **Depends on:** M5. Parallel with M7.

### [ ] M6-T1 Command history
- Per `device_id` + transport, persisted (last 500, dedupe consecutive) in store; ↑/↓ navigation, Ctrl+R reverse search.
- Setting to disable history persistence.
- **Tests:** navigation, dedupe, cap.

### [ ] M6-T2 Multiple sessions (tabs)
- Terminal sessions belong to one device; with M9 they nest inside that phone's device tab.
- Tabs with independent scrollback and running command; ⌘T new, ⌘W close (cancels running stream with confirm).

### [ ] M6-T3 ANSI rendering
- Parse SGR color codes (e.g. `anser` or small in-house parser); strip other escape codes safely; no HTML injection (render as React text nodes).
- **Tests:** colors, malformed sequences, `<script>` in output rendered as text.

### [ ] M6-T4 Interrupt & timeouts
- Ctrl+C cancels running command; optional per-command timeout setting; show "cancelled" footer.

### [ ] M6-T5 Virtualized scrollback
- 10 000 lines per session, smooth at 1 000 lines/s.

### [ ] M6-T6 Saved snippets
- Name, command, transport, `requires_confirmation` flag; run from sidebar or ⌘K palette; confirmation dialog shows the exact command.

### [ ] M6-T7 Copy/export
- Copy block output, copy all, export session to `.txt` via save dialog.

### [ ] M6-T8 Interactive PTY sessions (required by M11)
- SSH transport only: request a PTY (`russh` pty-req), forward keystrokes and terminal resize, render with a terminal emulator component (`xterm.js` via `@xterm/xterm`).
- Used for interactive setup wizards (e.g. Hermes configuration) and full-screen tools; PTY output is never persisted to history or logged.
- **Tests:** keystroke/resize forwarding with in-process SSH server; output not persisted.

## Exit check
- [ ] Long-running stream (`ping -c 100`) doesn't block UI; history survives restart.
