# Architecture Decision Records

Format: Context → Decision → Consequences. Status: Accepted / Proposed / Superseded.

---

## ADR-001 Tauri 2 + Rust + React/TS/Vite + Tailwind
**Status:** Accepted
**Context:** Brief mandates the stack. Need native-feeling, small macOS app with a privileged backend able to spawn `adb`.
**Decision:** Tauri 2, Rust 2021 edition, React 19 + TypeScript strict, Vite, Tailwind CSS v4.
**Consequences:** Small binary, Rust owns all I/O. Requires Rust toolchain + Xcode CLT for development.

## ADR-002 npm is the only package manager
**Status:** Accepted
**Context:** Repo had no package manager; brief lists `npm` commands.
**Decision:** npm with committed `package-lock.json`. No pnpm/yarn/bun.
**Consequences:** CI uses `npm ci`.

## ADR-003 Shell out to the `adb` binary through one ProcessRunner
**Status:** Accepted
**Context:** Brief requires using `adb` (not bundled) and centralization.
**Decision:** `AdbClient` builds argv; `ProcessRunner` is the only spawner. Never invoke a host shell (`sh -c`).
**Consequences:** Fully mockable; no host-side injection. Speaking the adb server protocol directly remains possible later behind the same client API.

## ADR-004 Termux access via SSH over `adb forward` (first), Control API (later)
**Status:** Accepted (pending M0 spike confirmation)
**Context:** `adb shell` runs as uid `shell` and cannot access Termux files or binaries (see BRAINSTORM §2).
**Decision:** `TermuxSshTransport`: Termux runs `sshd` bound to 127.0.0.1:8022; app runs `adb -s S forward tcp:0 tcp:8022`, connects with `russh` using an app-generated ed25519 key. Later `ApiTransport` uses the same forward technique.
**Consequences:** One-time Termux setup (install openssh, add public key). Nothing listens on the LAN. Plain `adb shell` remains available as the "Android shell" transport.

## ADR-005 Streaming via `tauri::ipc::Channel`, state changes via events
**Status:** Accepted
**Context:** Need ordered, high-throughput streams scoped to one consumer, plus app-wide notifications.
**Decision:** Per-stream `Channel<T>` for command output and logs; `app.emit` for `device://*` events. Rust batches log lines (≤50 ms or ≤500 lines).
**Consequences:** No polling for logs; explicit `cancel_stream` lifecycle.

## ADR-006 Generate TypeScript types from Rust with `ts-rs`
**Status:** Accepted
**Context:** Strongly typed IPC without drift.
**Decision:** `#[derive(TS)]` on IPC types, exported to `src/types/generated/` by `cargo test`. CI fails if generated files differ.
**Consequences:** TS types are never hand-edited. (`tauri-specta` reconsidered when it reaches stable for Tauri 2.)

## ADR-007 Device discovery via `adb track-devices`
**Status:** Accepted
**Context:** Brief forbids aggressive polling; we need fast disconnect detection.
**Decision:** Long-lived `adb track-devices -l` stream parsed in `adb/tracker.rs`; on stream failure, restart it with backoff and fall back to `adb devices -l` polling every 5 s.
**Consequences:** Near-instant state updates; one persistent child process.

## ADR-008 Settings with `tauri-plugin-store`
**Status:** Accepted
**Context:** Need persisted typed config, not ad-hoc files.
**Decision:** Single `settings.json` via store plugin, loaded into `AppState` as `AppConfig`, versioned schema with migrations. Secrets (API token) in macOS Keychain via `keyring` crate.
**Consequences:** One source of truth; secrets not in plain JSON.

## ADR-009 Frontend state with zustand; no router library
**Status:** Accepted
**Context:** Small desktop app with six views.
**Decision:** zustand stores; view switching via a typed `route` value in a store.
**Consequences:** Fewer dependencies; easy to test.

## ADR-010 Distribution: Developer ID signed + notarized DMG, outside the Mac App Store
**Status:** Accepted
**Context:** App Sandbox forbids spawning arbitrary binaries like `adb`.
**Decision:** Hardened runtime, Developer ID signing, notarization, DMG + updater artifacts on GitHub Releases.
**Consequences:** Requires Apple Developer account and CI secrets (see RELEASE.md).

## ADR-011 Hermes Control API binds to 127.0.0.1 with optional token
**Status:** Proposed (M8)
**Context:** Brief §17: never expose publicly; LAN requires auth.
**Decision:** Default bind `127.0.0.1`, accessed via `adb forward`. Binding to any other interface requires `HERMES_CONTROL_TOKEN`; server refuses to start otherwise. Implementation language: Python (stdlib + `aiohttp` or similar) since Python is already present for Hermes.
**Consequences:** Zero LAN exposure by default.

## ADR-012 Multiple phones, one tab per phone
**Status:** Accepted
**Context:** User wants to manage several phones at once. Wireless ADB serials (`ip:port`) change whenever the port changes, so they can't identify a phone.
**Decision:** Stable `device_id` (`ro.serialno` → `ro.boot.serialno` → `android_id`) keys profiles, tabs, history and SSH host keys. Backend `DeviceRegistry` holds an isolated `DeviceSession` per phone. Per-phone `DeviceProfile` overrides global Hermes/Termux defaults. UI: header tab per phone + Overview grid. Broadcast actions only for predefined actions/snippets, with explicit multi-select and confirmation.
**Consequences:** All milestones must avoid single-device assumptions (frontend stores keyed by `device_id` from M2). Settings schema gains `devices` map with migration. Resource limits needed for many concurrent streams.

## ADR-013 Hermes environment is separate from transport
**Status:** Accepted
**Context:** Hermes may run natively in Termux or inside a `proot-distro` rootfs. Running `command -v hermes` / `python --version` / start commands from the Termux shell gives wrong results for proot installs.
**Decision:** `HermesEnvironment` is `Termux` or `ProotDistro { distro }` and is independent of `TransportKind`. Commands are wrapped with `proot-distro login <distro>` and shell-escaped values. Process/state probes read from the Termux side using the resolved `containers/<distro>/rootfs` or `installed-rootfs/<distro>` layout; `detect_hermes` searches Termux and installed distros, then runs the configured version command inside each candidate.
**Consequences:** Commands are editable and entered as if inside the selected environment. Custom environment templates are not supported yet; Termux and proot-distro cover the target install.

## ADR-014 In-app provisioning with a one-time keystroke handoff
**Status:** Proposed (pending M0-S4)
**Context:** A fresh phone has no Termux and no SSH. ADB alone cannot run commands as the Termux user, and `RUN_COMMAND` requires a Termux setting that can't be written from ADB.
**Decision:** Install Termux via `adb install` (checksum-verified download), grant storage permission via `pm grant`, push a secret-free `bootstrap.sh` + public key to `/sdcard/Download/hacc/`, and type one command into Termux with `adb shell input text`. The script installs and starts sshd on 127.0.0.1; all later steps run over SSH. Progress before SSH is reported through a status file read with `adb shell cat`. Everything after bootstrap is driven by editable recipes; each step is check-then-act and resumable.
**Consequences:** Phone must be unlocked with Termux in front for about a minute. Keystroke injection is the only UI automation in the app and is limited to this single step. Termux and plugins must come from one source (signature compatibility).

## ADR-015 App-provided gateway supervisor for proot installs
**Status:** Accepted
**Context:** Default target is Debian in proot-distro with the official Hermes installer. Hermes' `gateway install/start/stop` rely on systemd (absent in proot), and Hermes' own restart paths (`hermes gateway restart`, chat `/restart`, `hermes update`, watchdog exit 75) exit and expect a supervisor to relaunch the process.
**Decision:** The app installs a small Termux-side supervisor script that runs `hermes gateway run` inside Debian, relaunches it with backoff unless a stop flag is set, and is itself started detached (`nohup setsid`) and by Termux:Boot. Stop = flag + SIGTERM; graceful restart = SIGUSR1 (Hermes drains turns) then relaunch. In detached mode, restart sends SIGTERM and starts a fresh gateway directly.
**Consequences:** Hermes restart/update flows work as on a server. The supervisor is versioned and rewritten by the app when its template changes. Users should decline Hermes' offers to install a system service inside proot.

## ADR-016 macOS first, Windows + Linux in v1.1, portable core from day one
**Status:** Accepted
**Context:** Brief targets macOS; users also want Windows and Linux. Tauri 2, Rust crates (`russh`, `tokio`, `keyring`) and ADB are cross-platform; differences are in paths, process spawning, secret stores, shortcuts, webviews and packaging/signing.
**Decision:** Ship macOS 1.0 first. From M0 keep the core portable: CI on all three OSes, Tauri path API only, `Mod` shortcut abstraction, all OS-specific code in `platform/` behind a `Platform` trait. Windows (NSIS/MSI, Authenticode) and Linux (AppImage/.deb/.rpm) ship in M12 as v1.1.
**Consequences:** Small upfront cost (3-OS CI, platform module). Windows needs a code-signing certificate; Linux needs per-desktop QA. Mac App Store remains out of scope (ADR-010).

## ADR-017 Primary host for public release downloads
**Status:** Proposed
**Context:** Signed desktop bundles need stable public URLs. The release workflow already assembles platform assets, checksums, updater metadata, and signatures into one GitHub Release draft. Cloudflare R2 would add separate storage credentials and mirror consistency/rollback work.
**Decision:** Use GitHub Releases as the canonical download host for the first public release. Consider Cloudflare R2 only if a custom domain, independent retention, or measured bandwidth/availability need justifies a mirror. If added, mirror exact signed assets and verification files; do not rebuild or re-sign them.
**Consequences:** No new hosting service or secrets are needed to begin distribution. GitHub remains the source of truth; any mirror requires byte-parity, signature, caching, and rollback checks. Public release stays blocked until the existing dependency-policy, signing, and platform QA gates pass.
