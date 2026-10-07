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
- Capabilities: `core:default`, `dialog:allow-save`, `updater:default`, and `process:allow-restart` for signed update installation. Settings persistence stays in Rust; no `shell:*`, broad `fs:*`, or clipboard plugin permissions.
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

## 6. Strict Dependency Remediation

The license allowlist and advisory checks remain unchanged. No advisory ignores,
license exceptions, relabeled third-party licenses, or unchecked source substitutions
are approved. A backport must retain upstream notices and have a reproducible source,
patch provenance, regression coverage, and native platform validation.

HTTP downloads and the updater use `native-tls`: platform certificate verification
and hostname checking remain enabled. macOS uses Security Framework, Windows uses
Schannel, and Linux uses OpenSSL and the host's CA store. This removes bundled
`webpki-roots` / `webpki-root-certs` and their rejected CDLA-Permissive-2.0 license.
Updater artifact signatures and provisioning SHA-256 verification are unchanged.
Linux builders require OpenSSL development files; runtime hosts need current CA
certificates. Locally trusted enterprise roots now follow OS trust policy.

Remaining blockers, verified from the resolved dependency graph on 2026-10-07:
- `tauri-build` / `tauri-codegen` require `tauri-utils`' HTML manipulation support,
  which brings `dom_query`, `cssparser`, `selectors`, and MPL-2.0 dependencies.
  Disabling optional app features does not remove this build-time path.
- Tauri's directory lookup brings MPL-2.0 `option-ext`; GTK's system-dependency
  tooling brings `target-lexicon` under Apache-2.0 WITH LLVM-exception.
- Linux GTK3 requires GLib `0.18`; `0.18.5` is the latest published compatible
  release checked. The `VariantStrIter` fix is in GLib `0.20+`, which is not a
  drop-in dependency upgrade for the GTK3 stack. `glib-macros` also retains the
  unmaintained `proc-macro-error` dependency.

Strict next steps are to replace noncompliant dependency implementations or obtain
an upstream permissively licensed alternative, and port the GTK3 stack to maintained
bindings or maintain verified GLib/macro backports. Security backports do not change
the licenses of copied code. A Tauri 3 prerelease/GTK4 migration is a separate
framework/toolchain/API change, not an automatic security patch; it also needs a
fresh license-graph check and does not by itself prove the HTML-parser licenses are
resolved. Preserve the current blocked release gate until remediation is verified.
