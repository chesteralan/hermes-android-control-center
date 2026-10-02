# M12 — Cross-Platform Release (Windows + Linux)

**Goal:** Ship the same app on Windows and Linux as v1.1, after the macOS 1.0. **Depends on:** M10. ADR-016.

Earlier milestones already keep the core portable (see "Groundwork"), so this milestone is mostly platform polish, packaging, signing and QA.

## Groundwork in earlier milestones (must already hold)
- M0: CI builds and tests on `macos-latest`, `windows-latest`, `ubuntu-latest` (Linux needs `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`).
- M1: all app paths via Tauri path API (`app_config_dir`, `app_data_dir`, `app_log_dir`); shortcuts use a platform modifier (`Mod` = ⌘ on macOS, Ctrl elsewhere); `ProcessRunner` hides console windows on Windows.
- M2: ADB locator has per-OS candidate lists; `adb.exe` handled.
- M4: SSH key file protected per OS.
- Platform-specific code lives only in `src-tauri/src/platform/{macos,windows,linux}.rs` behind one `Platform` trait.

## Platform adaptation

### [ ] M12-T1 ADB detection per OS
| OS | Candidates (after configured path and PATH) |
|---|---|
| Windows | `%LOCALAPPDATA%\Android\Sdk\platform-tools\adb.exe`, `%ANDROID_HOME%`/`%ANDROID_SDK_ROOT%`, Scoop (`%USERPROFILE%\scoop\apps\adb\current\platform-tools\adb.exe`, `\scoop\shims\adb.exe`), Chocolatey (`C:\ProgramData\chocolatey\bin\adb.exe`), winget links (`%LOCALAPPDATA%\Microsoft\WinGet\Links\adb.exe`) |
| Linux | `/usr/bin/adb`, `/usr/local/bin/adb`, `~/Android/Sdk/platform-tools/adb`, `$ANDROID_HOME`/`$ANDROID_SDK_ROOT`, `/opt/android-sdk/platform-tools/adb`, snap/flatpak shims |
- Install hints in `AdbNotFound` are per OS.
- **Tests:** candidate list per OS (cfg-gated + pure function taking an OS enum).

### [ ] M12-T2 Process spawning on Windows
- `CREATE_NO_WINDOW` creation flag for every spawned process (no flashing consoles).
- Kill process tree on cancel (Job Object) so `adb` children don't linger.
- Line splitting handles `\r\n`.
- **Tests:** CRLF parsing in all ADB parsers; cancel leaves no orphan (Windows CI integration test with a dummy child).

### [ ] M12-T3 Secrets & key files
- `keyring`: Windows Credential Manager, Linux Secret Service. If Secret Service is unavailable (headless/minimal WM): clear warning + opt-in encrypted file fallback (key derived from a user passphrase), never silent plaintext.
- SSH private key: Windows → restrict ACL to current user; Linux/macOS → `0600`.
- **Tests:** permission helpers per OS; fallback path requires explicit opt-in.

### [ ] M12-T4 UI & shortcuts
- Render `Mod` as ⌘ / Ctrl in menus, tooltips, cheat sheet; avoid conflicts with Ctrl+C in terminal (copy only with selection, otherwise interrupt).
- Native menus per OS (Windows/Linux: menu in window; macOS: app menu).
- Font stack per OS for monospace; verify WebView2 and WebKitGTK rendering of tables, virtualized lists, xterm.js.
- **Tests:** shortcut label mapping; visual snapshot review per OS (manual).

### [ ] M12-T5 Linux specifics
- Wayland + X11 sanity (window, clipboard, file dialogs via portal).
- Document `adb` udev rules only for USB (not needed for wireless).
- `WEBKIT_DISABLE_DMABUF_RENDERER=1` fallback documented for GPU/driver rendering issues.

## Packaging, signing, updates

### [ ] M12-T6 Windows bundles
- NSIS installer (per-user, no admin) + MSI (for managed installs); WebView2 bootstrapper embedded.
- Authenticode signing (OV/EV certificate or Azure Trusted Signing) in CI; timestamping.

### [ ] M12-T7 Linux bundles
- AppImage (primary, updater-capable), `.deb`, `.rpm`; Flatpak optional later.
- GPG-sign AppImage / release checksums.

### [ ] M12-T8 Updater per platform
- `latest.json` with `darwin-universal`, `windows-x86_64`, `linux-x86_64` (+ `aarch64` where built); `.deb`/`.rpm` users update via package (in-app notice only).

### [ ] M12-T9 Release workflow matrix
- Tag → build on 3 OSes → sign each → upload all artifacts + `SHA256SUMS` to one GitHub Release.

## Docs & QA

### [ ] M12-T10 Docs
- Setup guide: ADB install per OS (winget/Scoop/SDK; apt/dnf/pacman/SDK), dev prerequisites per OS.
- Troubleshooting: Windows firewall prompt for adb, SmartScreen, Linux keyring, WebKitGTK rendering.
- Root README: platform support table + download links.

### [ ] M12-T11 QA matrix
- Windows 10 22H2 + Windows 11; Ubuntu 22.04/24.04 (GNOME, Wayland + X11), Fedora latest (KDE).
- Run M10-T5 resilience matrix and M3-T10 first-milestone checklist on each; performance budgets (TESTING.md §6) per OS.

## Exit check
- [ ] Signed Windows installer and AppImage install and auto-update without warnings (SmartScreen reputation permitting).
- [ ] Pair, connect, Hermes control, terminal, logs, provisioning wizard work identically on all three OSes.
- [ ] No orphan `adb` processes after quit on any OS.
- [ ] Tag `v1.1.0`.
