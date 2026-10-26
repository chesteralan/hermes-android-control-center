# Testing Strategy

## 1. Principles
- No physical device required for any automated test.
- All `adb` interaction goes through `ProcessRunner` → replaced by `FakeRunner` with recorded fixtures.
- Parsers are pure functions → table tests + property tests (never panic on any input).
- UI tests mock `@tauri-apps/api/core` (`invoke`, `Channel`) — never the network.

## 2. Pyramid

| Layer | Tooling | Scope |
|---|---|---|
| Rust unit | `cargo test`, `proptest`, `tokio::test(start_paused)` | argv builders, parsers, error mapping, backoff, status parsing, batcher |
| Rust integration | `src-tauri/tests/`, `FakeRunner`, in-process `russh` server | DeviceManager, tracker, HermesManager, transports, command handlers |
| Contract | ts-rs diff check, JSON Schema (M8) | Rust ↔ TS ↔ Python type agreement |
| Frontend unit/component | Vitest, Testing Library, user-event | components, stores, ipc wrappers |
| E2E (smoke) | WebDriver via `tauri-driver` is not supported on macOS → manual QA matrix + Playwright against Vite build with mocked IPC | critical flows |
| Manual | QA matrix (M3-T10, M10-T5) | real phone(s) |

## 3. Fixtures

```
src-tauri/tests/fixtures/
├── adb/
│   ├── version.txt
│   ├── devices_none.txt / devices_usb.txt / devices_wireless.txt
│   ├── devices_unauthorized.txt / devices_offline.txt / devices_multiple.txt
│   ├── track_frames.bin
│   ├── connect_ok.txt / connect_refused.txt / connect_already.txt / connect_timeout.txt / connect_auth_failed.txt
│   ├── pair_ok.txt / pair_wrong_code.txt
│   ├── mdns_check.txt / mdns_services.txt / mdns_services_qr_pairing.txt
│   ├── getprop.txt / dumpsys_battery.txt / df_data.txt / meminfo.txt / cpuinfo.txt / ip_wlan0.txt
│   └── device_info_sectioned.txt
├── logcat/threadtime.txt
└── hermes/
    ├── pgrep_running.txt / pgrep_none.txt
    ├── status_output.txt
    ├── detect_termux_only.txt / detect_proot_debian.txt / detect_none.txt
    ├── pm_termux_fdroid.txt / pm_termux_play.txt / pm_termux_missing.txt
    └── logs_sample.txt (redacted)
└── provision/
    ├── recipes/default.toml / invalid_*.toml
    ├── bootstrap_status_*.txt (progress markers: started / pkg / sshd / done / error)
    ├── install_ok.txt / install_failed_incompatible.txt / install_failed_user_restricted.txt
    └── dumpsys_window_locked.txt / dumpsys_window_termux_focused.txt
```

Rules: redact real serials/IPs (use 192.0.2.0/24 TEST-NET), no personal data, note Android version + adb version in a header comment file `fixtures/README.md`.

## 4. Required test coverage (from brief §20)

**Rust**
- [ ] ADB command construction (every builder)
- [ ] Device parsing (`devices -l`, track frames)
- [ ] ADB output parsing (connect/pair/getprop/battery/df/meminfo/cpuinfo/ip)
- [ ] Device state handling (transitions, multi-device)
- [ ] Multi-device isolation (concurrent sessions, serial rebind to same `device_id`, profile override merge, one device failing doesn't block others)
- [ ] Connection retry logic (backoff schedule, give-up, manual disconnect suppression)
- [ ] Hermes status parsing (running/stopped/unknown/partial)
- [ ] Error handling (stderr → AppError mapping, serialization)

**React**
- [ ] Device status rendering (all fields, Unknown)
- [ ] Connection states (connected/offline/unauthorized/connecting/reconnecting/gave up)
- [ ] Log viewer (append, auto-scroll, pause, clear, search, filter, copy)
- [ ] Terminal (submit, streaming, stderr, exit code, cancel, history)
- [ ] Error states (ErrorPanel details)
- [ ] Device tabs (per-tab state preserved, close confirmation), Overview grid, confirm dialogs name the target phone
- [ ] Provisioning wizard (step states, consent gating, phone-action banner, resume)

**Provisioning (Rust)**
- [ ] Plan runner: skip Done, stop on failure, resume, cancel, persisted progress
- [ ] Recipe parse/validate; ABI → APK asset; checksum mismatch aborts
- [ ] `input text` escaping; bootstrap status parsing; Android-version-specific settings commands

## 5. CI gates (every PR)
Run on a matrix of `macos-latest`, `windows-latest`, `ubuntu-latest` (ADR-016). Platform-specific tests are `#[cfg(target_os)]`-gated; pure per-OS logic (ADB candidate lists, shortcut labels) is tested on every OS via an OS enum.
```
npm ci
npm run lint
npm run typecheck
npm test -- --run --coverage
npm run test:release
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
npm run gen:types && git diff --exit-code src/types/generated
npm run tauri build -- --debug   (PR) / full release build (tags)
```
Coverage targets: Rust parsers/backoff/status ≥ 90 %, overall Rust ≥ 70 %, frontend ≥ 70 %.

## 6. Performance budgets (verified in M10)
| Metric | Budget |
|---|---|
| Cold start to interactive | < 1.5 s |
| Idle CPU (connected, no streams) | < 1 % |
| Idle memory | < 200 MB |
| Log throughput | 2 000 lines/s for 60 s, no frame > 100 ms |
| Terminal throughput | 1 000 lines/s smooth |
| Device info refresh | < 2 s over Wi-Fi |
| Orphan processes after quit | 0 |
| 3 phones, all streaming logs (500 lines/s each) | UI responsive, no frame > 100 ms, memory < 400 MB |

### Local results — 2026-10-13, macOS arm64, no phone attached

Measured against the unsigned release `.app` (`tauri build`) with
`scripts/perf-probe.sh` (cold-start proxy: process spawn → first on-screen
layer-0 window via `CGWindowListCopyWindowInfo`; idle sampled after 15 s):

| Row | Result |
|---|---|
| Cold start to interactive | Pass — 0.4–0.7 s wall across 3 launches (budget 1.5 s) |
| Idle CPU (no phone attached) | Pass — 0.0 % (5 samples × 2 s) |
| Idle memory (no phone attached) | Pass — 73–91 MB RSS, settled ≈ 73 MB |
| Orphan processes after quit | Pass after fix — the probe found one leaked `adb track-devices -l` child per launch on SIGTERM; SIGTERM/SIGINT now take the graceful quit path and `pgrep` shows 0 leftovers |

Still requires a connected phone (blocked rows): idle CPU/RAM with a device
connected and no streams, log/terminal throughput, device-info refresh, and the
3-phone streaming row.

### Connected-device results — 2026-10-19, macOS arm64, 2 phones attached

Packaged release build; OnePlus CPH2239T2 (Android 11) + Samsung SM-T225
(Android 14), both wireless. Harness notes: AX automation drives the packaged
app (WebKit AXPress dispatches the tab strip's `role="tab"` press to its child
× button, and backgrounded pixel clicks no-op on WKWebView — tab switches were
exercised via Overview→Open and Device-list row select; a human tab-click check
is pending).

| Row | Result |
|---|---|
| Idle CPU (connected, no streams) | Pass — 0.0 % across 5 samples × 2 s (budget 1 %) |
| Idle memory (connected, no streams) | Pass — 71–72 MB RSS (budget 200 MB) |
| Device info refresh over Wi-Fi | Pass — 0.41 s click→rendered value (budget 2 s); the Dashboard storage row rounds to whole GiB, so sub-GiB deltas are invisible without a larger change |
| Terminal throughput | Pass — device generated 20 000 lines in 0.21 s; fully streamed and rendered within 0.8 s end-to-end (budget 1 000 l/s), AX stayed responsive during |
| Log throughput (2 000 l/s, 60 s) | Source-limited — phone-side shell `log` generators reach ≈36 l/s each, ≈300 l/s total at 30-way parallelism; the app held 0.27–0.37 s AX latency at that rate with no growth. Full budget still covered only by the synthetic M7-T10 benchmark (120k lines/60 s, no frame > 100 ms) |
| 3 phones streaming | Blocked — 2 real phones (stub-based 3-phone row recorded under M10-T5) |
| Orphans after quit | ⌘Q path fails with real streams — tracker + both logcat children orphaned (reproduced 3×; see M10-T5); SIGTERM probe path remains clean |

## 7. Cross-Platform QA (M12)

Hosted [run 37611442113](https://github.com/chesteralan/hermes-android-control-center/actions/runs/37611442113)
passed macOS, Windows, and Ubuntu automated build/test jobs, including Windows ACL,
Job Object cancellation, and encrypted-vault checks. Its dependency-policy job was
blocked by license/advisory findings at the time; the policy was reviewed and updated
on 2026-10-12 (see [SECURITY.md §6](SECURITY.md#6-strict-dependency-remediation)),
`cargo deny check` now passes locally, and the Gitleaks shallow-clone failure was
fixed with `fetch-depth: 0`. The overall run is expected green on the next push.

No interactive Windows/Linux runtime result is recorded yet. Run every row on each environment;
attach OS/build/WebView version, hardware, Android/ADB versions, measurements, and
sanitized screenshots/logs. Also run [M10 resilience checks](tasks/M10-production.md)
and [M3 first-workflow QA](tasks/M3-basic-terminal-logs.md).

| Environment | Install/Update | Core Workflows | Rendering/Shortcuts | Resilience/Performance |
|---|---|---|---|---|
| Windows 10 22H2 x86_64 | Pending | Pending | Pending | Pending |
| Windows 11 x86_64 | Pending | Pending | Pending | Pending |
| Ubuntu 22.04 GNOME X11 | Pending | Pending | Pending | Pending |
| Ubuntu 22.04 GNOME Wayland | Pending | Pending | Pending | Pending |
| Ubuntu 24.04 GNOME X11 | Pending | Pending | Pending | Pending |
| Ubuntu 24.04 GNOME Wayland | Pending | Pending | Pending | Pending |
| Fedora current KDE | Pending | Pending | Pending | Pending |

- Verify signed NSIS/MSI publisher/timestamp and per-user NSIS without elevation;
    test missing WebView2 installation and organizational MSI policy.
- Verify GPG key fingerprint, AppImage/checksum signatures, fresh install and signed
    update from the preceding release; reject modified signatures. `.deb`/`.rpm` must
    show a package-manager notice and must not call the binary updater.
- Pair/connect/reconnect at least two phones; run Hermes control, PTY, logs, sessions,
    and the provisioning wizard. Cancel a long command, drop Wi-Fi, sleep/wake, then quit.
- On Windows, run Job Object and ACL tests in CI and inspect process lists after quit.
    Test fresh ADB server creation, reuse of an existing Android Studio server, and server
    restart. The shared daemon may persist intentionally; no app-owned clients may remain.
- Test native credential storage with a running/unlocked keyring and unavailable/locked
    Secret Service. Check explicit encrypted fallback consent, locked startup, wrong
    passphrase, corruption, and no secrets in diagnostics or settings.
- Review tables, virtualized terminal/log lists, xterm selection/interrupt behavior,
    native menu/Mod labels, clipboard, portal dialogs, resize and scaled displays under
    WebView2/WebKitGTK. Test Wayland tray fallback and the DMABUF rendering workaround.
- Record every Section 6 performance budget per OS; never infer it from unit tests.
