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
| Path traversal on export | Log, terminal, and diagnostics destinations | Path only from native save dialog; shared Rust writer rejects relative, empty, and root paths before writing |
| Tampered APK during provisioning | Download of Termux / Termux:Boot | HTTPS from official source only, SHA-256 verified against source metadata, abort on mismatch; when a GitHub release publishes no asset digest, the app uses the release's checksum file if present, otherwise records the first-download digest in the install receipt and reports trust-on-first-download in the step output; consent dialog shows source + version |
| Leaks via shared storage | Bootstrap handoff in `/sdcard/Download/hacc/` | Only script + **public** key placed there; deleted after success |
| Unwanted system changes | Battery / phantom-process settings | Per-item consent listing exact commands; optional items never block |
| Secrets captured during setup | Hermes configure step | PTY output not persisted or logged; config editor masks secret-looking values, writes files with mode 600 |
| Recipe runs malicious commands | Imported recipes | Imported recipes shown in full before first run; consent lists every command |
| Supply chain | npm/crates | lockfiles committed, `npm audit`, `cargo audit`, `cargo deny`, Dependabot |
| Tampered updates | Updater | Tauri updater signature verification + notarized builds |

## 3. Tauri hardening
- CSP: `default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src ipc: http://ipc.localhost`.
- `withGlobalTauri: false`.
- Capabilities: `core:default`, the three notification permissions used by `nativeNotifications.ts` (`allow-is-permission-granted`, `allow-request-permission`, `allow-notify`), `updater:default`, and `process:allow-restart` for signed update installation. File dialogs open from Rust (`app.dialog()`), which is not capability-gated, so no `dialog:*` permission is granted to the webview. Settings persistence stays in Rust; no `shell:*`, broad `fs:*`, or clipboard plugin permissions. The exact set is locked by the `capability_grants_match_reviewed_set` test (reviewed 2026-10-14).
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
- [x] No secrets in repo (2026-10-14: CI Security audits job — Gitleaks with `fetch-depth: 0` — passes on `main`; local full-history scan found no leaks)
- [x] CSP verified in built app (2026-10-14: policy present in the packaged binary, served as the `Content-Security-Policy` header on `tauri://` HTML responses; frontend has no remote fetch/XHR/WebSocket/eval usage; locked by the `csp_matches_reviewed_policy` test)
- [x] Capabilities reviewed against this doc (2026-10-14: exact grant set locked by `capability_grants_match_reviewed_set`)
- [x] `cargo audit` / `npm audit` clean or exceptions documented (2026-10-14: zero production npm vulnerabilities; RustSec exceptions documented in §6; both enforced in the CI Security audits job)
- [ ] Termux services verified bound to 127.0.0.1
- [x] Diagnostics bundle redaction verified (2026-10-14: real archive exported from the packaged app with seeded Bearer/JWT/API-key/Telegram-bot/password/secret values and IPs — all rendered `[REDACTED]`/`[IP REDACTED]`, `adbPath` → `[REDACTED PATH]`)
- [x] Vulnerability reporting process in root `SECURITY.md` (private GitHub vulnerability reporting + contact fallback documented)

## 6. Strict Dependency Remediation

The license allowlist and advisory checks were updated on 2026-10-12 with an
explicit, reviewed decision:

- `MPL-2.0` is allowed for dependencies. MPL-2.0 is file-level copyleft: the
  obligation attaches only to modified MPL files, and this project does not
  modify the rejected crates (`cssparser`, `cssparser-macros`, `dtoa-short`,
  `option-ext`, `selectors` — all transitive via Tauri's HTML-parser path and
  directory lookup).
- `Apache-2.0 WITH LLVM-exception` is allowed for dependencies
  (`target-lexicon`, via GTK's `system-deps` tooling). The license is
  permissive and OSI-approved.
- `RUSTSEC-2024-0370` (`proc-macro-error` unmaintained) is ignored with a
  written justification in `deny.toml`: it is a build-time-only transitive
  dependency of `glib-macros 0.18` in Tauri's GTK3 stack with no compatible
  upgrade, and the advisory reports no vulnerability.

Remediation pass on 2026-10-26 after new RustSec advisories (published
2026-10-09 through 2026-10-23) failed the CI security job:

- **Upgraded (vulnerabilities fixed):** `crypto-bigint` 0.7.5 → 0.7.10
  (`RUSTSEC-2026-0387`, also yanked), `ctutils` 0.4.2 → 0.4.6
  (`RUSTSEC-2026-0385`), `tokio` 1.53.1 → 1.54.1 (`RUSTSEC-2026-0386`
  unsoundness), and the app's own `zip` 2.4.2 → 9.1.2 (`RUSTSEC-2026-0367`
  / `0368` / `0369` for our diagnostics/APK archives) — the zip 2 → 9 API
  surface used here (`ZipWriter`, `SimpleFileOptions`, `ZipArchive`) compiled
  unchanged and all 360 tests pass.
- **Ignored with justification (`deny.toml`, mirrored in the CI audit step's
  `ignore` input):** `RUSTSEC-2026-0367/0368/0369` still apply to `zip 4.6.1`,
  pulled in by `tauri-plugin-updater 2.13.1` (`zip ^4`). No release on the `^4`
  line is patched — the fix exists only in `zip >= 9.0.0` — and every published
  plugin version (2.13.x, 2.14.0, 3.0.0-alpha) still requires `^4`. The updater
  extracts only signature-verified release archives. Drop the ignore when the
  plugin line moves to a patched zip.
- **Ignored with justification:** `RUSTSEC-2026-0370/0371/0372`
  (`zvariant` / `zbus_names` / `zvariant_derive` unmaintained — merged into
  `zbus 6.0.0` upstream). Transitive via `notify-rust` in the pinned
  `tauri-plugin-notification 2.4.0`; no compatible upgrade exists and the
  advisories report no vulnerability.

No relabeled third-party licenses, unchecked source substitutions, or
dependency forks were adopted. `RUSTSEC-2024-0429` (`glib` unsoundness) remains
a visible audit warning: it is an unsoundness advisory, not a vulnerability,
and fixing it requires the GTK3/GLib line described below. A backport must
retain upstream notices and have a reproducible source, patch provenance,
regression coverage, and native platform validation.

HTTP downloads and the updater use `native-tls`: platform certificate verification
and hostname checking remain enabled. macOS uses Security Framework, Windows uses
Schannel, and Linux uses OpenSSL and the host's CA store. This removes bundled
`webpki-roots` / `webpki-root-certs` and their rejected CDLA-Permissive-2.0 license.
Updater artifact signatures and provisioning SHA-256 verification are unchanged.
Linux builders require OpenSSL development files; runtime hosts need current CA
certificates. Locally trusted enterprise roots now follow OS trust policy.

Dependency-graph findings, verified from the resolved dependency graph on 2026-10-07
and resolved by policy decision on 2026-10-12:
- `tauri-build` / `tauri-codegen` require `tauri-utils`' HTML manipulation support,
  which brings `dom_query`, `cssparser`, `selectors`, and MPL-2.0 dependencies.
  Disabling optional app features does not remove this build-time path.
  **Resolved:** `MPL-2.0` is now on the allowlist under the terms above.
- Tauri's directory lookup brings MPL-2.0 `option-ext`; GTK's system-dependency
  tooling brings `target-lexicon` under Apache-2.0 WITH LLVM-exception.
  **Resolved:** both license expressions are now on the allowlist.
- Linux GTK3 requires GLib `0.18`; `0.18.5` is the latest published compatible
  release checked. The `VariantStrIter` fix is in GLib `0.20+`, which is not a
  drop-in dependency upgrade for the GTK3 stack. `glib-macros` also retains the
  unmaintained `proc-macro-error` dependency (`RUSTSEC-2024-0370`, ignored with
  justification in `deny.toml`).
  **Open:** `RUSTSEC-2024-0429` (`glib` unsoundness) remains an audit warning
  requiring a verified GLib backport or a maintained-bindings migration.

`cargo deny check` (advisories, bans, licenses, sources) passes with the updated
policy. Long-term next steps are porting the GTK3 stack to maintained bindings or
maintaining verified GLib backports. Security backports do not change the licenses
of copied code. A Tauri 3 prerelease/GTK4 migration is a separate
framework/toolchain/API change, not an automatic security patch; it also needs a
fresh license-graph check.
