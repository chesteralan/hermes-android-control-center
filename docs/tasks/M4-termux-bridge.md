# M4 — Termux Bridge

**Goal:** Run commands as the Termux user via SSH over `adb forward` (ADR-004). **Depends on:** M3, M0-S1.

## Backend

### [x] M4-T1 Port forwarding manager
- `AdbClient::forward(serial, "tcp:0", "tcp:<remote>")` → parse allocated local port; track per (serial, remote) and remove on disconnect/app exit (`forward --remove`).
- Re-establish after reconnect.
- **Tests:** argv, port parsing, cleanup on drop.

### [x] M4-T2 SSH key management
- Generate ed25519 keypair on first use in app data dir (private key restricted to the current user: `0600` on macOS/Linux, owner-only ACL on Windows via `platform::restrict_file`); expose public key via `get_termux_public_key`.
- Never log key material.

### [~] M4-T3 `TermuxSshTransport`
- Implemented with russh 0.63; in-process SSH server test still TODO — verified only against a real phone.
- `russh` client to `127.0.0.1:<forwarded>`; user from config (Termux ignores username but keep it configurable); host key pinned on first connect (TOFU) per `device_id` (not serial, which changes for wireless), mismatch → error with details.
- `execute`: exec channel, collect stdout/stderr/exit status, timeout.
- `stream`: exec channel → `StreamEvent`s; cancel closes channel (sends signal where supported).
- Login shell: wrap as `bash -lc '<escaped>'` only if configured, using a tested `shell_escape` function.
- Connection pooling: one session per device, reconnect lazily.
- **Tests:** `shell_escape` table tests; transport against an in-process `russh` test server (no phone).

### [x] M4-T4 Termux health check
- `check_termux(serial)` returns checklist: Termux app installed (from M2-T5b) · adb connected · forward ok · sshd reachable · auth ok · `$PREFIX` present · `proot-distro` available + installed distros.
- Python/Hermes checks are **not** done here — they depend on the Hermes environment and live in M5-T1b.
- Each item: ok / failed + fix hint.
- Maps failures to `TermuxUnavailable { reason }`.

### [ ] M4-T5 Optional bootstrap via RUN_COMMAND
- Deferred: M0-S1 RUN_COMMAND spike not run yet (writes to the phone).
- Only if M0-S1 succeeded: "Start sshd in Termux" button sends the intent. Otherwise hide and show manual instruction.

### [x] M4-T6 Transport selection
- `execute_command`/`stream_command` gain `transport: TransportKind` (`AdbShell` | `TermuxSsh`); `HermesConfig.transport` default `TermuxSsh`.

## Frontend

### [x] M4-T7 Termux setup wizard
- Steps with copy buttons:
  1. `pkg install openssh`
  2. Append public key to `~/.ssh/authorized_keys`
  3. `sshd -o ListenAddress=127.0.0.1` (port 8022)
  4. Optional: `termux-wake-lock`, `pkg install termux-services && sv-enable sshd`
  5. Verify (runs `check_termux`)
- **Tests:** renders checklist states.

### [x] M4-T8 Terminal transport selector
- Toggle "Android shell | Termux"; badge on each output block showing which transport ran it.

## Exit check
- [ ] `whoami` in Termux mode returns Termux uid; in Android shell returns `shell`.
- [ ] Streaming `tail -f` in Termux works and cancels cleanly.
- [ ] No listener on the phone's LAN interface (`ss -ltn` shows 127.0.0.1 only).
