# Open Work

Consolidated snapshot of unfinished task headings and milestone exit checks as of 2026-10-13. The linked milestone task files remain the source of truth; update this index when their status changes.

Status: `[ ]` open, `[~]` in progress, `[!]` blocked. Items marked blocked need new evidence or a supported environment before implementation can proceed.

## Open Findings — Wave 1 live multi-device QA (2026-10-19)

Recorded from live QA on two connected phones (OnePlus CPH2239, Android 11; Samsung SM-T225, Android 14) against the packaged release build. Each finding has reproduction evidence in the referenced task file. Status: 1–4 fixed and live-verified on 2026-10-19 (uncommitted); 5–7 open.

1. **[FIXED] Device profiles are keyed by address, not stable `device_id` (M9-T2 regression).** After the config v2→v3 migration, `settings.json` `devices` contained nine profiles keyed like `192.168.0.107:43237`; neither real phone had a profile under `ro.serialno`. **Fix:** `remember_address_for(address, device_id)` keys profiles by identity when known; `AppConfig::rekey_profile` moves/merges address-keyed ghosts onto the stable id; `AppState::adopt_profile_identity` is invoked from identity-learning sites (`learn_device_id`, `get_device_info`) and persists. **Verified live:** v2→v3 migration → learn → rekey → both phones keyed by their hardware serials with current `last_address`. 8 unit tests (config×6, state×2). Unmatchable stale-port ghosts remain until Forget (see finding 5).
2. **[FIXED] Reconnect supervisor and manual Retry failed to rebind after mDNS-alias loss.** Root cause: finding 1 — `reconnect_target` reads `profiles[device_id].last_address`, which missed for real devices and fell back to the dead serial. **Verified live:** transport drop → Reconnecting 1/8 → auto-Connected in ~6 s with no manual Connect.
3. **[FIXED] Log stream state went stale after serial rebind.** Root cause: `LogcatSource` had no restart loop (child exit silently ended the receiver while the UI kept the stale `streamId`) and captured its serial at construction. **Fix:** respawn-with-backoff loop (1→8 s cap, reset after 30 s healthy) + per-spawn serial resolution via `DeviceRegistry::serial_for(device_id)`. **Verified:** unit tests (restart keeps receiver alive; respawn uses rebound serial) + live: stream continued across drop/reconnect and the running child's argv re-resolved from mDNS alias to `ip:port`.
4. **[FIXED] `⌘Q` quit leaked adb children.** Root cause: tao 0.37's macOS backend has no `applicationShouldTerminate` veto, so ⌘Q → NSApp terminate killed the process before any cleanup; the `ExitRequested` gate fires too late to defer. **Fix:** the main app menu's Quit item is now our own `QUIT_ID` menu item with the ⌘Q accelerator, routing through the graceful `request_quit` (the same path the tray Quit already used); an `ExitRequested` handler in `lib.rs` defers tauri-internal exit() routes until cleanup finishes (`AppLifecycle::exit_cleanup_finished` gate + tests). **Verified live:** user ⌘Q with tracker running → `track-devices exited` logged, 0 orphans (pre-fix: leak on 4/4 attempts). Note: quit now takes ~1–3 s while children are reaped.
5. **[FIXED] Remembered-address rot.** Root cause: `known_addresses` and ghost profiles kept one entry per wireless-debugging port rotation (35037→37809→40347 observed), all retried at every launch. **Fix:** `remember_address_for` supersedes other ports on the same host in both `known_addresses` and the profile map (newest port wins, other hosts untouched); `rekey_profile` also drops same-host stale ghosts when adopting identity; `migrate_v2_to_v3` dedupes to the newest port per host before migrating. Pre-existing stale entries from other hosts remain until Forget (Settings → Connection/Devices). 3 unit tests.
6. **[FIXED] Tray/menu-bar tooltip counts transports, not phones.** **Fix:** `distinct_phone_count` dedupes by learned `device_id` (serial fallback) before the tooltip count. Unit test covers IP+alias of one phone counting as 1.
7. **[FIXED] View-state durability gaps vs M9 exit check.** **Fix:** per-tab route memory (`tabRoutes`) persists to `localStorage` (`hacc.tabRoutes`, validated on load, global views excluded) and the startup route restores the active tab's remembered view (`startupRoute`); the Logs viewer per-device UI state (search/filters/pause) moved from component `useState` to a module-level zustand store (`stores/logsUi.ts`) so it survives navigating away and back. 4 route tests + 1 unmount/remount logs test; the in-view tab-switch behavior is unchanged.

Automation artifacts (not app defects): WebKit `AXPress` on the tab strip's `role="tab"` div dispatches to its first button child (the × close button), and backgrounded pixel clicks no-op on this WKWebView — direct tab-click behavior needs a human verification (pending). cua-driver also lost its Accessibility grant during the session (re-grant in System Settings if further GUI automation is needed).

Automation artifacts (not app defects): WebKit `AXPress` on the tab strip's `role="tab"` div dispatches to its first button child (the × close button), and backgrounded pixel clicks no-op on this WKWebView — direct tab-click behavior needs a human verification (pending).

## Recently Completed

- [x] Local QA pass — M10-T5 resilience rows, M10-T4 audit, M16-T3 download docs (2026-10-14): **T5** — adb server kill recovers (tracker respawned, app stable), quit during 3 live log streams leaves 0 orphans, and 3 phones streaming survive one dropping and auto-reconnecting without touching the others, all driven by the new committed `scripts/stub-adb.py` stub (framed `track-devices`, sectioned device-info shell, live logcat, invocation log proving serial-scoped targeting). **T4** — keyboard/AX/contrast audit on the packaged app: visible focus ring, ⌘K filter, About-dialog Tab trap (wrap verified on the real-click path), full ARIA exposure (dialog semantics, tab group names, aria-live toasts, aria-keyshortcuts), every text pair passes WCAG AA; one finding fixed — form-control boundaries were 1.3:1 vs the required 3:1, now `--color-border-strong` (3.1:1) on inputs/textareas/selects. **M16-T3** — README `## Download` section with platform/arch/min-OS table, per-platform install + update guidance (deb/rpm package-manager note), SHA256SUMS/GPG/Authenticode/notarization verification anchors, and the stable-vs-prerelease updater policy; the literal `releases/latest` link waits on the first signed release.

- [x] M10-T2 security-review closeout — CSP review, capability review, CI scanners, diagnostics archive (2026-10-14): The built-app CSP review confirmed the reviewed policy is embedded in the packaged binary and served as the `Content-Security-Policy` header on `tauri://` HTML, with a frontend audit showing no remote fetch/XHR/WebSocket/eval/`dangerouslySetInnerHTML` usage (locked by the new `csp_matches_reviewed_policy` test). The capability review found `dialog:allow-open`/`dialog:allow-save` were granted but unused — all file dialogs open from Rust, which is not capability-gated — so both were removed; the exact grant set is locked by the new `capability_grants_match_reviewed_set` test and SECURITY.md §3/§5 were synced. CI's Security audits job (Gitleaks, npm audit, RustSec, cargo-deny) passes on `main`. M10-T6 closed: a real archive exported from the packaged app with seeded Bearer JWT / `api_key` / `sk-` / Telegram-bot / password / secret values and two IPs redacted everything to `[REDACTED]`/`[IP REDACTED]` (incl. `adbPath` → `[REDACTED PATH]`).

- [x] Configure Hermes is automatic now (M11-T12): when the wizard's Configure Hermes step pauses on an unconfigured phone it renders an in-app form (model provider/name/base URL/API key + optional Telegram/Discord/Slack tokens with allow-lists) instead of the manual `proot-distro login …` terminal instructions. Submitting applies the setup over SSH via the previously unexposed `apply_hermes_setup` backend command (new `ipc.applyHermesSetup` binding) and re-runs the step, whose existing check verifies completion — secrets are written only to the phone's `~/.hermes/.env` (mode 600). The terminal handoff, interactive PTY, and Hermes `--portal` remain as a "Run manually instead" fallback; systemd/allow-list guidance stays visible in the panel. Four wizard tests cover the panel, apply + auto-resume, validation, and the failure path.

- [x] M10 release-ticket verification pass (T10–T13, T15, fresh-Mac exit check, 2026-10-13): the root README now embeds a sanitized first-run screenshot captured from the packaged app using a stub ADB path (no real device data), and all `docs/guides/` links were audited. Fresh-profile QA with an isolated `HOME` shows the window in ~0.7 s, writes startup logs, and exits with zero leftovers on SIGTERM. Local unsigned DMG bundling is reproducible (10.96 MiB). Confirmed the repository still has no CI secrets (`gh secret list` empty) — RELEASE.md §3 names everything to provision for signing, notarization, and the updater key (T10–T13 remain blocked on those credentials). Note: one phone (CPH2239) is now visible on the shared ADB server; M9-T9 still needs a second phone.

- [x] [M9-T6 — Overview view](tasks/M9-multi-device.md): a new global Overview route (nav + ⌘K palette) shows a status card per phone — alias/color swatch, model, connection state, battery and storage snapshot, Hermes gateway state, and the latest reconnect attempt. Each card polls device info and Hermes status on its own 10 s cadence, so one slow phone only spins its own card; "Open" selects that phone's tab (restoring its remembered view) and leaves the global Overview for the per-device Device view. Four tests cover mixed availability (connected with data, info error, still-pending phone never gating another), tab linking, the reconnect line, and the empty state. M9-T9 (live multi-device QA) remains blocked — no phones attached.

- [x] [M9-T7 — Independent reconnect verification](tasks/M9-multi-device.md): reconnection now prefers each phone's remembered profile `last_address` (port changes no longer strand a phone on its stale serial), the frontend reconnect map is keyed by stable `device_id` so `device://reconnect` status follows the right tab across rebinds, and a fake-clock test proves two concurrent failing loops advance independently — phone A giving up while phone B is still retrying, with events attributed per device. Manual Retry re-arms only the selected phone (frontend test).

- [x] [M9-T8 — Destructive actions and limited broadcast](tasks/M9-multi-device.md): every destructive confirmation now names the target phone via a shared `deviceIdentity` helper — DeviceList disconnect, Hermes stop/restart/update, terminal session close and snippet run, provisioning consent/reset/Termux uninstall, and the Control API install review all show alias, model, and address. Limited broadcast landed on saved snippets: with two or more phones the run dialog gains an "Also run on" multi-select (predefined snippets only, one confirmation), each selected phone gets its own terminal session block, and one phone's failure never cancels or mislabels the others (per-block exit/error + toast summary).

- [x] [M10-T3 — Performance budgets](tasks/M10-production.md): local macOS probe (`scripts/perf-probe.sh` + `scripts/winprobe.swift`) records cold start 0.4–0.7 s to first window, idle CPU 0.0 %, idle RAM ≈ 73 MB with no phone, and 0 orphan processes after quit. The probe found and the fix landed for a SIGTERM leak of one `adb track-devices -l` child per launch — SIGTERM/SIGINT now take the graceful quit path. Connected-idle, throughput, device-info, and 3-phone rows remain blocked on a phone; procedure and partial results recorded in TESTING.md §6.

- [x] [M9-T5 — Per-tab terminal and log isolation](tasks/M9-multi-device.md): the log viewer's search input/query, regex/case toggles, match cursor, level filter, pause snapshot, and auto-scroll now live in a per-device record, so switching tabs never leaks viewer state (buffers were already per phone from M9-T1; terminal sessions/scrollback were too — now regression-tested with interleaved two-device streams). AppConfig gains `limits.maxLogLines` (serde default 20,000, validated non-zero, no settings UI yet) and the log store ring buffer honors it. `devices.disconnect` now stops the phone's frontend log stream before the backend disconnect (which cancels its PTY sessions, SSH, and reconnect), and device tabs gained a close (×) for wireless phones: the confirm dialog names the phone and explains that its terminal sessions and log streams end — hidden for USB, mirroring Disconnect. Six new tests cover two-device interleaving (terminal + logs), per-phone pause/search state, cancel-on-close isolation, the hidden USB close, and the configured buffer limit.

- [x] [M9-T4 — Device tab bar](tasks/M9-multi-device.md): the phone tab strip now renders in the app header (full width, hidden when a single phone is known) with per-phone color swatch, alias/model fallback, connection-state dot, and an overflow menu (Retry for disconnected phones — the reconnect path also serves as Connect — Disconnect with a confirm dialog naming the target phone, Open profile). Selecting a tab focuses the phone and restores that phone's remembered view via per-tab route memory in `stores/route.ts` (global views like Settings are untouched); tab order and the last-selected tab persist in localStorage by `device_id`; a wireless port change rebinds to the same tab. 22 local tests cover mixed states (connected/reconnecting/offline/disconnected/unknown), order and selection persistence, per-tab view restore, and port-change rebind.

- [x] M9 local quality gates restored (typecheck, ESLint, Prettier all pass): fixed the pre-existing M9-T2/T3 fallout — `TermuxConfig` exported from the types barrel, `set_device_profile` typed as `Partial<DeviceProfileUpdate>` (matching the Rust `#[serde(default)]` partial-update contract), `AppConfig` test fixtures gained `devices: {}`, unused imports/parameters removed, and `DeviceTabBar` tab-order sync rewritten as a render-phase adjustment instead of `setState` in an effect. `src/types/generated` re-exported from Rust via `npm run gen:types` so `DeviceProfileUpdate` carries its type imports.

- [x] "Install Hermes Agent" no longer dies at "bootstrap Python lookup failed" on android/proot (M11-T11): uv 0.12.3's `python find --managed-python` cannot see freshly installed managed interpreters under proot (the flag alone reproduces the miss; plain find works — verified on-device with RUST_LOG=debug). The step now retries a failed install once with the installer's pin-verified uv binary wrapped by a shim that strips only that flag (uv moved to `uv.real`, shim execs it unchanged), after deleting version-alias symlinks in the managed python dir. Confirmed on-device: the installer's exact lookup exits 0 through the shim.

- [x] "Install Hermes Agent" now runs automatically for interactive recipes (M11-T11): after the consent dialog, the reviewed installer is downloaded inside the environment over the SSH bridge, its SHA-256 is printed into the streamed output, and it is executed with `--non-interactive` (the Hermes installer's documented flag for skipping the setup/gateway stages that need a terminal — those are handled by the Configure Hermes step, whose phone-action message now names the full `proot-distro login <distro> -- bash` entry command). The manual "View installer script" / "Run reviewed installer" terminal handoff is gone.

- [x] "Install Linux environment" step hardened (M11-T10): invalid `proot-distro list --installed` replaced with a direct rootfs probe that detects a complete install (`<rootfs>/etc`) across v4, legacy, and v5 containers layouts; an incomplete rootfs left by a failed install is removed before reinstalling; rootfs download failures from the default easycli.sh mirror (known ISP/DPI interference, termux/proot-distro#643) are retried once from the official GitHub release via `PD_OVERRIDE_TARBALL_URL`; probe failures containing `CANNOT LINK EXECUTABLE` (Termux partial-upgrade breakage, e.g. lscpu/libsmartcols desync) get `pkg upgrade -y` guidance appended.

- [x] "Install Hermes Agent" no longer dead-ends on the native APT recipe with "requires adding its signed repository manually": the step now runs the official installer `script_url` inside Termux over SSH whenever one is configured (native and proot recipes share the path), the bundled `termux-native-apt` recipe sets `script_url`, and a native recipe without a script URL now points at `hermes_install.script_url` in the recipe editor. (M11-T11)

- [x] Termux bridge card: Verify auto-runs on page load (once, when the phone is connected), and a new "Reset host key & repair bridge" button (`repair_termux_ssh_bridge`) forgets the pinned host key and re-runs the bootstrap handoff (`pkg install openssh`, authorize the app key, start sshd) in one click, then re-verifies. The bootstrap flow is now a shared function used by both the wizard and the repair command. (M4-T7, M11-T7)

- [x] Bootstrap `fail: sshd: runsv not running` fixed: `bootstrap.sh` detaches `service-daemon start` (setsid when available), waits for runit to supervise `sshd` before `sv-enable`, and falls back to a direct `sshd` start; the Termux-packages step got the same detached start + tolerant `sv-enable`. (M11-T7)

- [x] Wizard no longer dead-ends on "Termux SSH host key changed" after its own Termux (re)install: the InstallTermux step and the confirmed `uninstall_incompatible_termux` flow now drop the pooled SSH session and forget the pinned TOFU host key, so the next Connect step re-verifies and re-pins automatically. (M11-T3/T4)

- [x] Proot recipes now have a "Grant Termux storage access" wizard step after Launch Termux: auto-types `termux-setup-storage`, checks `appops get com.termux MANAGE_EXTERNAL_STORAGE` / `READ_EXTERNAL_STORAGE` grant state, and falls back to a resume prompt. Skipped (Done) for native APT recipes. (`provision/types.rs`, `plan.rs`, `executor.rs`; M11-T6b)
- [x] Bootstrap `trap: ERR: bad trap` fixed: `bootstrap.sh` now uses a POSIX `trap ... EXIT` for the error marker so `sh /sdcard/Download/hacc/bootstrap.sh` works when Termux's `sh` is not bash. (M11-T7)
- [x] Termux install on the GitHub source no longer fails with "GitHub release does not provide a SHA-256 digest": the app now falls back to the release's SHA256SUMS-style checksum asset, then to trust-on-first-download with the digest recorded in the install receipt and reported in the step output; a present-but-invalid digest is still a hard error. SECURITY.md §2 and M11-T4 updated.
- [x] `tasks/M9-multi-device.md` written (the roadmap referenced it but the file was missing); M9 tasks/exit checks now exist and the M11-T2/T14 dependency is documented.
- [x] APK download safety limit raised from 100 MiB to 200 MiB (`MAX_APK_MIB` in `provision/apk.rs`); the error message is now derived from the constant; M11 task doc updated.
- [x] CI Security-job Gitleaks failure fixed: `actions/checkout@v4` now uses `fetch-depth: 0` so gitleaks-action can resolve its commit range; local full-history scan found no leaks.
- [x] Dependency policy resolved (owner-approved 2026-10-12): `deny.toml` allows `MPL-2.0` and `Apache-2.0 WITH LLVM-exception` and carries a justified `RUSTSEC-2024-0370` ignore; `cargo deny check` passes all categories. SECURITY.md §6, M10-T2, and M12 updated.
- [x] Prettier gate fixed: `.prettierignore` now excludes `android/hermes-control/.venv`, generated schemas, and pytest caches; 9 source/docs files formatted; `npm run format:check` passes.

- [x] [M10-T1 — Recoverable SSH-key initialization failure](tasks/M10-production.md): poisoned initialization returns an error without creating a key; key reload/permissions behavior remains tested.
- [x] [M10-T1 — Observable logging setup and reload failures](tasks/M10-production.md): file/subscriber/reload errors are reported instead of silently ignored; console fallback is retained and broader startup QA remains open.
- [x] [M10-T1 — Consistent shell-exit and device-error classification](tasks/M10-production.md): command-not-found exits stay ordinary command results; missing-device streams report device errors.
- [x] [M10-T1 — Final streamed ADB stderr classification and details](tasks/M10-production.md): split-chunk errors without a final newline retain device-error classification and output; ordinary command exits remain unchanged.
- [x] [M10-T1 — Terminal input errors precede connection setup](tasks/M10-production.md): blank execute/stream requests are rejected before ADB/SSH/API setup; broader error audit remains open.
- [x] [M10-T1 — Provisioning-run lock poisoning returns an error](tasks/M10-production.md): IPC reports poisoned state and cleanup failures are logged; the wider error audit remains open.
- [x] [M10-T1 — Shared export destination validation and write-error coverage](tasks/M10-production.md): log, terminal, and diagnostics exports reject relative/empty/root paths; broader error audit and native-dialog QA remain open.
- [x] [M10-T6 — Quoted diagnostics secret redaction and synthetic ZIP regression coverage](tasks/M10-production.md): JSON/log secrets, escaped quoted values, Bearer headers, and both IP-consent settings verified; packaged-app archive QA completed 2026-10-14 (see Recently Completed), closing the ticket.
- [x] [M3-T10 — macOS live terminal and logcat QA](tasks/M3-basic-terminal-logs.md): stdout/stderr, exit status, streamed output, Cancel, and Logs Start/Stop/Clear verified on CPH2239.
- [x] M0-S3 verified the sanitized `track_frames.bin` header (`0074` = 116 payload bytes) and added an exact-length parser assertion. The broader capture ticket remains in progress.
- [x] [M13-T5 — Device-scoped restoration and documentation](tasks/M13-chat-sessions.md): persistence, unavailable-device behavior, environment separation, privacy, and profile limitations are tested/documented.
- [x] [M13-T2 — Typed Rust session access](tasks/M13-chat-sessions.md): scoped commands, bounded pagination/responses, token-verified loopback access, and malformed/truncated JSON errors are implemented and tested; one Hermes v0.21.4 live check is recorded.
- [x] M13 session API limits are documented and regression-tested: 20-session recent list, 1–500 messages per page, 2 MiB list response, and 8 MiB transcript response.
- [x] M13-T1 local startup test rejects an unrelated loopback HTTP service without spawning Hermes; on-device conflict behavior remains open.
- [x] [M10-T16 — Contribution guide, security reporting policy, and MIT license](tasks/M10-production.md).
- [x] M13-T6 local macOS validation passes (182 Rust tests, 92 frontend tests, lint, typecheck, formatting, and Clippy); platform/device QA remains open.
- [x] M10-T1 removed a guarded production chat `exit_code.unwrap()`; the wider panic/error-propagation audit remains in progress.
- [x] [M10-T14 — Synchronized application versions and Keep a Changelog](tasks/M10-production.md)
- [x] [M8-T1 — Hermes Control API service skeleton](tasks/M8-control-api.md): Python package, aiohttp entrypoint, config/install script, and loopback/token bind guard.
- [x] [M8-T2–T9 — Endpoints, schemas, supervision, desktop transports, app install, and Settings switch](tasks/M8-control-api.md)
- [x] [M14-T1–T2 — Tray menu, close/minimize-to-tray, restore, and explicit quit verified on macOS](tasks/M14-tray-lifecycle.md)
- [x] [M11-T8 — Provisioning reuses the SSH bridge](tasks/M11-provisioning.md)
- [x] [M11-T16 — Form/raw-TOML recipe editor, validation, duplication, import/export](tasks/M11-provisioning.md)
- [x] [M12-T1 — Per-OS ADB candidates and install hints](tasks/M12-cross-platform.md)
- [x] [M12-T10 — Desktop setup, development, security, and troubleshooting guide](tasks/M12-cross-platform.md)

## In Progress

- [~] [M10-T1 — Error audit](tasks/M10-production.md)
- [~] [M10-T2 — Isolated GLib backport passes optimized library tests and 241 Linux app tests/Clippy; dependency policy resolved 2026-10-12 and `cargo deny check` passes; CSP/capability/diagnostics/CI-scanner reviews completed 2026-10-14 — only the glib unsoundness remediation path remains](tasks/M10-production.md)
- [~] [M10-T4 — Keyboard command palette and shortcut guide](tasks/M10-production.md): local keyboard/AX/contrast audit done 2026-10-14 (focus ring, ⌘K filter, dialog Tab trap, ARIA exposure, all text AA); form-boundary contrast fixed via `--color-border-strong`; manual VoiceOver walkthrough and Windows/Linux shortcut checks remain.
- [~] [M10-T7 — Existing setup path; automatic wizard awaits M11](tasks/M10-production.md)
- [~] [M10-T8 — About, version, and native app menu](tasks/M10-production.md)
- [~] [M10-T9 — Tray status, named stop alerts, and persistent gateway timeline; packaged notification/runtime QA remains](tasks/M10-production.md)
- [~] [M10-T10–T13 — Signing, universal build, updater, and release workflow](tasks/M10-production.md)
- [~] [M10-T15 — Root README](tasks/M10-production.md)

## Blocked

- [!] [M4-T5 — Optional bootstrap via RUN_COMMAND](tasks/M4-termux-bridge.md): Android denied the intent because the ADB-shell sender lacks `com.termux.permission.RUN_COMMAND`; manual SSH setup remains supported.
- [!] M0-S3/S4 remaining fixture and provisioning acceptance: `adb devices -l` is empty; USB, unauthorized/offline, pairing failures, reset-phone install, and reboot captures require a connected Android phone.
- [!] M11 phone acceptance: factory setup, rerun/idempotency, interruption recovery, Termux:Boot, permission prompts, and on-phone cleanup require a connected Android phone.
- [~] M11 T2/T14 per-phone HermesConfig persistence: M9-T2 DeviceProfile schema and IPC are implemented (config v3, effective_config merge, CRUD commands, profile editor UI); the M11 wizard handoff (saving recipe-derived HermesConfig into the phone's profile) remains open.
- [!] M11 T2/T11 signed APT repository install: no verified repository URL exists, so the automated signed-repo flow (key URL + repo entry) remains unbundled. The wizard no longer blocks on this — the native recipe installs via the official `script_url` in Termux.
- [!] M13-T1/T6 live compatibility checks: only Hermes v0.21.4 has been tested on-device; upstream lists v0.21.6, but setting a supported range and verifying gateway-created sessions requires a phone. Windows/Linux session and performance QA also requires those platforms.
- [!] M14-T3/T4 Windows/Linux/Wayland tray and release QA: this workspace is on macOS; native runtime behavior cannot be certified without those desktop environments.
- [!] M12 native runtime/installer QA: Windows/Linux desktop hosts are unavailable; M12-T6/T7 additionally require Authenticode/GPG signing credentials.
- [!] M16-T1–T4 public release/download acceptance: requires signing credentials and a published release; optional R2 evaluation remains discretionary.

## Open Tickets

### M10 — Hardening & Production

- [~] [M10-T3 — Performance budgets](tasks/M10-production.md): connected-device rows measured 2026-10-19 — connected-idle 0.0 % CPU / 71–72 MB RSS (2 phones), device-info refresh 0.41 s (budget 2 s), terminal throughput 20 000 lines rendered in 0.8 s end-to-end (budget 1 000 l/s). Log-throughput row is source-limited: a phone-side shell generator tops out ≈300 l/s while UI latency stays 0.27–0.37 s; the 2 000 l/s budget remains covered only by the synthetic M7-T10 benchmark.
- [~] [M10-T5 — Resilience QA matrix](tasks/M10-production.md): real-phone rows verified 2026-10-19 — phone sleep/wake, Wi-Fi switch (loss detected <1 s, backoff recovery), wireless-debugging toggle with port change (33843→37809) rebinds to the same tab/device, and one phone dropping leaves the other streaming uninterrupted. Quit-during-stream with real logcat found the ⌘Q child-leak (finding 4). Termux-killed, Hermes-crash, and 3-real-phone rows remain open (bridge/phone availability).

### M9 — Multi-Device

Task breakdown written 2026-10-12; T1 backend isolation groundwork is complete (device_id keys in streams, sessions, effective config). T2 (DeviceProfile schema/persistence/IPC) and T3 (profile editor UI) are implemented and tested. T4 (device tab bar with per-tab views and localStorage persistence) is implemented and tested locally. T5 (per-tab log viewer state, limits config, tab close cleanup) is implemented and tested locally. T6 (Overview grid) is implemented and tested locally. T7 (reconnect isolation, profile-address targeting, device-keyed reconnect events) and T8 (phone-identifying confirmations, snippet broadcast) are implemented and tested locally. T9 live QA ran on 2026-10-19 with two connected phones — most rows pass, but it surfaced the profile-keying regression (finding 1) and reconnect/stream-rebind gaps (findings 2–3); the 3-phone row and the M11 handoff cross-check remain open.

- [~] [M9-T1 — Backend isolation audit and completion](tasks/M9-multi-device.md)
- [~] [M9-T2 — DeviceProfile schema, persistence, and effective config](tasks/M9-multi-device.md)
- [~] [M9-T3 — Profile editor UI](tasks/M9-multi-device.md)
- [~] [M9-T4 — Device tab bar](tasks/M9-multi-device.md)
- [~] [M9-T5 — Per-tab terminal and log isolation](tasks/M9-multi-device.md)
- [~] [M9-T6 — Overview view](tasks/M9-multi-device.md)
- [~] [M9-T7 — Independent reconnect verification](tasks/M9-multi-device.md)
- [~] [M9-T8 — Destructive actions and limited broadcast](tasks/M9-multi-device.md)
- [~] [M9-T9 — Multi-device QA (2 phones live 2026-10-19; 3-phone row pending)](tasks/M9-multi-device.md): two-phone rows verified live (tabs, per-device buffers/sessions, Overview cards, action targeting, confirmations naming the target, drop/reconnect isolation, wireless port-change rebind); findings 1–3, 7 came out of this pass. Remaining: 3-real-phone streaming row, human tab-click verification, M11 wizard profile handoff cross-check.

### M11 — New Phone Provisioning

Local foundations are implemented and tested (295 Rust, 134 frontend, 21 Python tests).
The task file lists remaining implementation details separately from phone-only QA;
this milestone is not complete.

- [~] [M11-T1 — In-step download/ADB/SSH cancellation added; disconnect recovery and phone QA remain](tasks/M11-provisioning.md)
- [~] [M11-T2 — Recipes; native repository and per-phone configuration remain](tasks/M11-provisioning.md)
- [~] [M11-T3 — Configurable threshold and typed Termux uninstall implemented; factory-phone QA remains](tasks/M11-provisioning.md)
- [~] [M11-T4 — Version checks, APK progress and common install-error guidance implemented; OEM/source and phone QA remain](tasks/M11-provisioning.md)
- [~] [M11-T5 — SDK-scoped settings commands implemented; phone permission and keyguard QA remain](tasks/M11-provisioning.md)
- [~] [M11-T6 — Wake/keyguard detection and bounded Termux focus wait implemented; phone QA remains](tasks/M11-provisioning.md)
- [~] [M11-T7 — All-outcome bootstrap cleanup added; retry/interruption phone QA remains](tasks/M11-provisioning.md)
- [~] [M11-T9 — Package/wake-lock output streams implemented; connected-phone QA remains](tasks/M11-provisioning.md)
- [~] [M11-T10 — Existing-distro reruns and partial-rootfs detection/guidance implemented; connected-phone repair QA remains](tasks/M11-provisioning.md)
- [~] [M11-T11 — Reviewed installer can be explicitly run; PATH discovery and phone QA remain](tasks/M11-provisioning.md)
- [~] [M11-T12 — Masked atomic YAML config fallback added; phone/install QA remains](tasks/M11-provisioning.md)
- [~] [M11-T13 — Autostart hook; phone activation/reboot QA remains](tasks/M11-provisioning.md)
- [~] [M11-T14 — Start/status verification; M9 profile persistence remains](tasks/M11-provisioning.md)
- [~] [M11-T15 — Device wizard, setup entry points, all-state/resume coverage added; responsive/runtime QA remains](tasks/M11-provisioning.md)

### M12 — Cross-Platform

Local implementation passes 237 Rust, 106 frontend, and 3 release-assembly tests.
Hosted macOS/Windows/Ubuntu build/test jobs now pass; the dependency-policy gate
was resolved on 2026-10-12 (see M10-T2). Interactive runtime, signing, and installed-update
acceptance remain open; no cross-platform release certification is claimed.

- [~] [M12-T2 — Job Objects/no-console clients; Windows runtime/orphan QA remains](tasks/M12-cross-platform.md)
- [~] [M12-T3 — Native stores/private keys/encrypted fallback; platform runtime QA remains](tasks/M12-cross-platform.md)
- [~] [M12-T4 — Modifier/font mapping and PTY copy; native visual QA remains](tasks/M12-cross-platform.md)
- [~] [M12-T5 — Wayland/portal/udev/DMABUF guidance; Linux runtime QA remains](tasks/M12-cross-platform.md)
- [!] [M12-T6 — NSIS/MSI configuration; Authenticode credentials and Windows install QA required](tasks/M12-cross-platform.md)
- [!] [M12-T7 — AppImage/deb/rpm configuration; GPG credentials and Linux install QA required](tasks/M12-cross-platform.md)
- [~] [M12-T8 — Consolidated metadata/package-manager guard; installed update QA remains](tasks/M12-cross-platform.md)
- [!] [M12-T9 — Three-OS build/test jobs pass; hosted signing remains (dependency-policy review resolved 2026-10-12)](tasks/M12-cross-platform.md)
- [~] [M12-T11 — QA matrix documented; native runtime/performance cells pending](tasks/M12-cross-platform.md)

### M16 — Public Download Distribution

GitHub Releases is the recommended primary host; the existing tag workflow assembles signed bundles as a draft. Public release remains gated on signing credentials and platform install/update QA; the dependency-policy gate was resolved on 2026-10-12. Cloudflare R2 is optional pending a concrete custom-domain, retention, or traffic requirement.

- [ ] [M16-T1 — Release publication prerequisites](tasks/M16-download-distribution.md)
- [ ] [M16-T2 — Verify the GitHub Release pipeline](tasks/M16-download-distribution.md)
- [~] [M16-T3 — Public download path and instructions](tasks/M16-download-distribution.md): README `## Download` section written 2026-10-14 (platform/min-OS table, per-platform install/update guidance incl. the deb/rpm package-manager note, SHA256SUMS/GPG/Authenticode/notarization verification with guide anchors, stable-vs-prerelease updater policy); the literal `releases/latest` link and published-page walkthrough wait on the first release (M16-T1/T2).
- [ ] [M16-T4 — Download and installation acceptance](tasks/M16-download-distribution.md)
- [x] [M16-T5 — R2 mirror evaluated and deferred; GitHub Releases remains primary](tasks/M16-download-distribution.md)

## Open Milestone Exit Checks

### M1 — Desktop Shell

- [!] [CI run and `v0.1.0-alpha.1` tag; release tagging is deferred with M10 signing readiness](tasks/M1-desktop-shell.md)

### M2 — ADB Core

- [!] [Live device info, mDNS discovery, rebuilt-app mDNS/IP disconnect, and Retry verified; fresh-phone pairing and wireless-toggle recovery need a connected phone](tasks/M2-adb-core.md)

### M3 — Basic Terminal + Live Logs

- [!] [Tag `v0.1.0` and attach the CI app; stable tagging depends on deferred M10 release readiness](tasks/M3-basic-terminal-logs.md)

### M5 — Hermes Management

- [~] [Action-sequence regressions covered; live Hermes detection/actions blocked until ADB and SSH are restored](tasks/M5-hermes.md)
- [!] [Verify supervisor recovery after app close and gateway exit; usable phone connection required](tasks/M5-hermes.md)

### M7 — Logs Complete

- [!] [Demonstrate brief §8 requirements on real Hermes logs; no Android device is connected](tasks/M7-logs.md)

### M8 — Hermes Control API

- [!] [Verify Hermes action state transitions and on-device API auth/status; requires a connected Hermes phone](tasks/M8-control-api.md)

### M10 — Hardening & Production

- [ ] [Fresh-Mac download, onboarding, and Hermes workflow QA](tasks/M10-production.md)
- [ ] [Tag `v1.0.0`](tasks/M10-production.md)

### M11 — New Phone Provisioning

- [!] [Factory-fresh Android 13/14+ provisioning run; requires a reset phone](tasks/M11-provisioning.md)
- [!] [Wizard rerun is idempotent; requires a provisioned phone](tasks/M11-provisioning.md)
- [!] [Interrupted wizard resumes successfully; requires a connected phone](tasks/M11-provisioning.md)
- [!] [Termux:Boot restores Hermes after reboot; requires a phone and reboot cycle](tasks/M11-provisioning.md)
- [!] [No bootstrap downloads or secrets remain on the phone; requires device inspection](tasks/M11-provisioning.md)

### M12 — Cross-Platform

- [!] [Windows and Linux install/update without warnings; requires those native hosts and signing credentials](tasks/M12-cross-platform.md)
- [!] [Core workflows match across all three OSes; Windows and Linux runtime QA remains](tasks/M12-cross-platform.md)
- [!] [No orphan ADB processes on any OS; Windows and Linux quit QA remains](tasks/M12-cross-platform.md)
- [!] [Tag `v1.1.0`; release is gated on signing and platform acceptance](tasks/M12-cross-platform.md)

### M13 — Chat Session History

- [!] [Browse app-created and gateway-created sessions; requires a connected Hermes phone](tasks/M13-chat-sessions.md)
- [x] [Document and test session API/page limits](tasks/M13-chat-sessions.md)
- [!] [Establish and test the supported Hermes version range; only v0.21.4 is live-verified](tasks/M13-chat-sessions.md)
- [!] [CI and connected-phone QA on all M12 platforms; Windows/Linux hosts and a phone are required](tasks/M13-chat-sessions.md)

### M14 — Tray & Window Lifecycle
- [x] [macOS QA: close/minimize hides the window and preserves active work](tasks/M14-tray-lifecycle.md)
- [x] [macOS QA: tray restore and explicit quit work](tasks/M14-tray-lifecycle.md)
- [x] [macOS QA: explicit quit cancels streams and reaps the logcat child](tasks/M14-tray-lifecycle.md)
- [!] [Behavior/fallback documented; Windows/Linux/Wayland runtime QA requires those desktop environments](tasks/M14-tray-lifecycle.md)
- [!] [Tag `v1.3.0`; gated on supported-platform runtime QA](tasks/M14-tray-lifecycle.md)

### M16 — Public Download Distribution

- [!] [Public, verified downloads available for each supported platform; release signing/publication prerequisites remain](tasks/M16-download-distribution.md)
- [!] [Clean install and upgrade acceptance completed; requires signed release artifacts and platform hosts](tasks/M16-download-distribution.md)
