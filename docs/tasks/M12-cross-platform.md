# M12 — Cross-Platform Release (Windows + Linux)

**Goal:** Ship the same app on Windows and Linux as v1.1, after the macOS 1.0. **Depends on:** M10. ADR-016.

Earlier milestones already keep the core portable (see "Groundwork"), so this milestone is mostly platform polish, packaging, signing and QA.

## Implementation Snapshot

2026-10-07: platform implementations, ADB candidates/hints, process-tree cancellation,
native credential backends, explicit passphrase-encrypted fallback, per-OS font/modifier
mapping, selection-aware PTY copy, bundle configs, consolidated updater metadata,
three-OS signing/release workflow, desktop guide, and QA matrix are implemented.

Local macOS verification: 237 Rust tests, 106 frontend tests, 3 Node release tests,
strict Clippy, ESLint, TypeScript, production web build, and Tauri platform-config schema
checks pass. Hosted [CI run 37611442113](https://github.com/chesteralan/hermes-android-control-center/actions/runs/37611442113)
on commit `8396633` passed all macOS, Ubuntu, and Windows build/test jobs, including
Windows Job Object/CRLF/ACL tests. Platform install/update and interactive performance
results are not inferred from these automated checks.

CI follow-up fixes: feature-branch triggers, exact binary fixture checkout, Unix-only
execution of the Android POSIX token-permission test, corrected audit crate paths and
Check Run permissions, and isolated Windows PowerShell module paths. Optional RSA
support was removed; Ed25519 SSH execution/streaming/cancellation still passes. Rust
audit reports zero vulnerabilities, with existing `proc-macro-error` unmaintained and
`glib` unsoundness warnings.

The dependency-policy job still blocks the overall run. It rejects existing
MPL-2.0 and Apache-2.0 WITH LLVM-exception dependencies, and reports
`RUSTSEC-2024-0370` (`proc-macro-error`). `RUSTSEC-2024-0429` (`glib`) remains an
audit warning requiring review. No license allowances or advisory ignores were added.
The project's existing MIT license is now declared in crate metadata. Completing this
gate requires replacements for rejected-license dependencies and verified upgrades
or backports for affected dependencies, not exceptions or disabling checks. The user
selected strict remediation: no policy exceptions. Both HTTP clients now use certificate-verified
native TLS, removing the CDLA-licensed embedded certificate-root packages from the
lockfile. Local HTTP/API and APK selector tests and Clippy pass. The mandatory Tauri
HTML-parser licensing and GTK3/GLib compatibility boundaries remain; see
[strict remediation evidence](../SECURITY.md#6-strict-dependency-remediation).

A mocked-IPC browser smoke run at 1280x900 and the native 900x600 minimum verified
vault consent, passphrase input clearing, locked state, package-update notice, and
control bounds. This is not WebView2/WebKitGTK or native secret-store certification.

External gates: interactive Windows/Linux hosts and a resolved dependency-policy gate; Apple/Authenticode/updater/GPG
credentials; publication of a trusted Linux signing-key fingerprint; signed installer
and previous-version update runs; the complete manual matrix below. No v1.1 tag or
public release was created. The shared ADB server is intentionally persistent and is
not an app-owned orphan; inspect client descendants separately during quit QA.

Implementation details and remaining checks:
- T1: pure per-OS candidate/hint tests cover SDK, Scoop/WinGet, Linux SDK/snap/Flatpak
	locations; configured path remains authoritative. Verify real package installations in QA.
- T2: process-wrap uses suspended-spawn Windows Job Objects and no-console flags,
	Unix groups, explicit timeout/cancel cleanup, and cancellation during output backpressure/EOF.
	Windows prepares the shared ADB daemon outside client jobs. Native dummy-descendant
	cleanup tests pass in CI; real ADB daemon restart and fresh-server startup QA remain.
- T3: Keychain/Credential Manager/Secret Service features are target-specific. SSH keys
	are restricted before writing. Encrypted storage requires checkbox consent and a
	passphrase; Argon2id/ChaCha20-Poly1305, locked reload, wrong-password/corruption tests
	pass. Existing native tokens are not migrated automatically. Native Windows ACL,
	key-file, encrypted-vault, and in-process SSH tests pass in CI; Linux desktop
	keyring/no-keyring interactive checks remain.
- T4/T5: native menus already use CmdOrCtrl; fonts, labels, PTY selection copy, Wayland
	fallback, portals/udev/DMABUF guidance are covered. Visual/clipboard/dialog QA remains.
- T6/T7: Windows NSIS/MSI embed the online WebView2 bootstrapper; NSIS is per-user.
	Linux produces AppImage/deb/rpm; PFX timestamping and detached GPG signing are
	configured, but real signing and install verification are blocked on credentials/hosts.
- T8/T9: one publisher produces tested signed-update metadata/checksums for built targets;
	Linux non-AppImage installs use package-manager notices. Hosted release runs and actual
	upgrade/reject-tampered-update acceptance remain.
- T10: setup/build/troubleshooting/support guidance is linked from the root README.
- T11: the Windows 10/11, Ubuntu 22.04/24.04 Wayland/X11, Fedora KDE matrix is recorded
	in TESTING Section 7; all native runtime/performance cells remain pending.

## Groundwork in earlier milestones (must already hold)
- M0: CI builds and tests on `macos-latest`, `windows-latest`, `ubuntu-latest` (Linux needs `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`).
- M1: all app paths via Tauri path API (`app_config_dir`, `app_data_dir`, `app_log_dir`); shortcuts use a platform modifier (`Mod` = ⌘ on macOS, Ctrl elsewhere); `ProcessRunner` hides console windows on Windows.
- M2: ADB locator has per-OS candidate lists; `adb.exe` handled.
- M4: SSH key file protected per OS.
- Platform-specific code lives only in `src-tauri/src/platform/{macos,windows,linux}.rs` behind one `Platform` trait.

## Platform adaptation

### [x] M12-T1 ADB detection per OS
| OS | Candidates (after configured path and PATH) |
|---|---|
| Windows | `%LOCALAPPDATA%\Android\Sdk\platform-tools\adb.exe`, `%ANDROID_HOME%`/`%ANDROID_SDK_ROOT%`, Scoop (`%USERPROFILE%\scoop\apps\adb\current\platform-tools\adb.exe`, `\scoop\shims\adb.exe`), Chocolatey (`C:\ProgramData\chocolatey\bin\adb.exe`), winget links (`%LOCALAPPDATA%\Microsoft\WinGet\Links\adb.exe`) |
| Linux | `/usr/bin/adb`, `/usr/local/bin/adb`, `~/Android/Sdk/platform-tools/adb`, `$ANDROID_HOME`/`$ANDROID_SDK_ROOT`, `/opt/android-sdk/platform-tools/adb`, snap/flatpak shims |
- Install hints in `AdbNotFound` are per OS.
- **Tests:** candidate list per OS (cfg-gated + pure function taking an OS enum).

### [~] M12-T2 Process spawning on Windows
- `CREATE_NO_WINDOW` creation flag for every spawned process (no flashing consoles).
- Kill process tree on cancel (Job Object) so `adb` children don't linger.
- Line splitting handles `\r\n`.
- **Tests:** CRLF parsing in all ADB parsers; cancel leaves no orphan (Windows CI integration test with a dummy child).

### [~] M12-T3 Secrets & key files
- `keyring`: Windows Credential Manager, Linux Secret Service. If Secret Service is unavailable (headless/minimal WM): clear warning + opt-in encrypted file fallback (key derived from a user passphrase), never silent plaintext.
- SSH private key: Windows → restrict ACL to current user; Linux/macOS → `0600`.
- **Tests:** permission helpers per OS; fallback path requires explicit opt-in.

### [~] M12-T4 UI & shortcuts
- Render `Mod` as ⌘ / Ctrl in menus, tooltips, cheat sheet; avoid conflicts with Ctrl+C in terminal (copy only with selection, otherwise interrupt).
- Native menus per OS (Windows/Linux: menu in window; macOS: app menu).
- Font stack per OS for monospace; verify WebView2 and WebKitGTK rendering of tables, virtualized lists, xterm.js.
- **Tests:** shortcut label mapping; visual snapshot review per OS (manual).

### [~] M12-T5 Linux specifics
- Wayland + X11 sanity (window, clipboard, file dialogs via portal).
- Document `adb` udev rules only for USB (not needed for wireless).
- `WEBKIT_DISABLE_DMABUF_RENDERER=1` fallback documented for GPU/driver rendering issues.

## Packaging, signing, updates

### [!] M12-T6 Windows bundles
- NSIS installer (per-user, no admin) + MSI (for managed installs); WebView2 bootstrapper embedded.
- Authenticode signing (OV/EV certificate or Azure Trusted Signing) in CI; timestamping.

### [!] M12-T7 Linux bundles
- AppImage (primary, updater-capable), `.deb`, `.rpm`; Flatpak optional later.
- GPG-sign AppImage / release checksums.

### [~] M12-T8 Updater per platform
- `latest.json` with `darwin-universal`, `windows-x86_64`, `linux-x86_64` (+ `aarch64` where built); `.deb`/`.rpm` users update via package (in-app notice only).

### [!] M12-T9 Release workflow matrix
- Tag → build on 3 OSes → sign each → upload all artifacts + `SHA256SUMS` to one GitHub Release.

## Docs & QA

### [x] M12-T10 Docs
- Setup guide: ADB install per OS (winget/Scoop/SDK; apt/dnf/pacman/SDK), dev prerequisites per OS.
- Troubleshooting: Windows firewall prompt for adb, SmartScreen, Linux keyring, WebKitGTK rendering.
- Root README: platform support table + download links.

### [~] M12-T11 QA matrix
- Windows 10 22H2 + Windows 11; Ubuntu 22.04/24.04 (GNOME, Wayland + X11), Fedora latest (KDE).
- Run M10-T5 resilience matrix and M3-T10 first-milestone checklist on each; performance budgets (TESTING.md §6) per OS.

## Exit check
- [ ] Signed Windows installer and AppImage install and auto-update without warnings (SmartScreen reputation permitting).
- [ ] Pair, connect, Hermes control, terminal, logs, provisioning wizard work identically on all three OSes.
- [ ] No orphan `adb` processes after quit on any OS.
- [ ] Tag `v1.1.0`.
