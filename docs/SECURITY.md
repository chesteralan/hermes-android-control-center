# Security

## 1. Assets
- Shell access to the Android device (adb shell uid) and Termux user (SSH/API).
- Hermes Agent and its credentials (e.g. bot tokens in Termux).
- ADB keys (`~/.android/adbkey`), app SSH private key, Control API token.
- Logs (may contain message content / secrets).

## 2. Threat model

| Threat | Vector | Control |
|---|---|---|
| Host command injection | Building `adb` invocations from user input | argv only, no host shell (ADR-003); address validation |
| Unintended device commands | Auto-executed or replayed commands | Terminal executes only on explicit submit; no auto-run of history/snippets; predefined actions use configured commands only |
| Destructive actions by mistake | Stop/Restart, snippets | Confirmation dialogs showing exact command |
| Action hits the wrong phone | Several phones connected | Target phone (alias + model + address) shown in prompt, tab and every confirm dialog; broadcast limited to predefined actions with explicit multi-select |
| LAN attacker reaches Termux services | sshd / Control API listening on Wi-Fi | Bind 127.0.0.1 + `adb forward`; API refuses non-loopback without token; verified in exit checks |
| ADB over Wi-Fi hijack | Open wireless debugging on hostile network | Docs: pair only on trusted networks, disable wireless debugging when not needed; Android's TLS pairing |
| SSH MITM | Forward to wrong port/device | Host key TOFU pinned per `device_id`; mismatch blocks |
| Secret leakage via logs | tracing output, diagnostics bundle | Commands at Debug only, output never logged, pairing code/tokens never logged, redaction in diagnostics |
| XSS in webview | Device output rendered as HTML | Render as text nodes only; no `dangerouslySetInnerHTML`; strict CSP |
| Over-privileged webview | Tauri plugins | Minimal capabilities; no `shell` plugin; fs scoped to user-selected paths |
| Path traversal on export | `export_logs` path | Path only from native save dialog; Rust validates absolute path |
| Tampered APK during provisioning | Download of Termux / Termux:Boot | HTTPS from official source only, SHA-256 verified against source metadata, abort on mismatch; consent dialog shows source + version |
| Leaks via shared storage | Bootstrap handoff in `/sdcard/Download/hacc/` | Only script + **public** key placed there; deleted after success |
| Unwanted system changes | Battery / phantom-process settings | Per-item consent listing exact commands; optional items never block |
| Secrets captured during setup | Hermes configure step | PTY output not persisted or logged; config editor masks secret-looking values, writes files with mode 600 |
| Recipe runs malicious commands | Imported recipes | Imported recipes shown in full before first run; consent lists every command |
| Supply chain | npm/crates | lockfiles committed, `npm audit`, `cargo audit`, `cargo deny`, Dependabot |
| Tampered updates | Updater | Tauri updater signature verification + notarized builds |

## 3. Tauri hardening
- CSP: `default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src ipc: http://ipc.localhost`.
- `withGlobalTauri: false`.
- Capabilities: `core:default`, `store:default`, `dialog:allow-open`, `dialog:allow-save`, `clipboard-manager:allow-write-text`, `updater:default` (M10). No `shell:*`, no broad `fs:*`.
- Hardened runtime entitlements: none beyond default unless required (document each).

## 4. Secrets storage
| Secret | Location |
|---|---|
| ADB keys | managed by adb in `~/.android/` (not touched) |
| App SSH private key | app data dir, owner-only (`0600` / Windows ACL) |
| Control API token | OS secret store via `keyring` (macOS Keychain, Windows Credential Manager, Linux Secret Service); opt-in passphrase-encrypted file if no Secret Service — never silent plaintext |
| Pairing code | memory only |
| QR pairing password | memory only; CSPRNG, new per attempt, expires after 2 min |

## 5. Release checklist
- [ ] No secrets in repo (`gitleaks` in CI)
- [ ] CSP verified in built app
- [ ] Capabilities reviewed against this doc
- [ ] `cargo audit` / `npm audit` clean or exceptions documented
- [ ] Termux services verified bound to 127.0.0.1
- [ ] Diagnostics bundle redaction verified
- [ ] Vulnerability reporting process in root `SECURITY.md`
