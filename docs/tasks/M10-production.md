# M10 — Hardening & Production (v1.0.0)

**Goal:** Ship a signed, notarized, documented, supportable 1.0. **Depends on:** M9, M11 (M8 may be marked optional for 1.0 if SSH transport is stable).

## Quality

### [~] M10-T1 Error audit
- Every brief §16 case reproduced and verified to show human message + Details: adb not installed, adb not found, device offline, unauthorized, connection refused, wireless debugging disabled, Termux unavailable, Hermes not found, command failed, network timeout.
- grep audit: no `unwrap()`/`expect()` in non-test Rust except proven-infallible with comment; no swallowed `Err`.
- **Progress:** A Rust payload regression test now checks that each listed failure category serializes a human-readable message and non-empty expandable details. Removed a guarded production `exit_code.unwrap()` from chat error formatting. End-to-end reproduction and the remaining non-test Rust panic/error-propagation audit remain open.
- **Progress:** The Rust error-payload test covers the ten listed ADB/Termux/Hermes failure categories and asserts each serialized payload has a human message and expandable details. The non-test panic/error-propagation grep audit and end-to-end reproductions remain open.
- **Progress (2026-10-08):** Log, terminal, and diagnostics exports now share an absolute-file destination guard and propagate filesystem write failures. Tests reject relative, empty, and root paths, verify exact exported bytes, and exercise a missing-parent write error. All 254 Rust tests, formatting, and strict Clippy pass. This closes the local export-path/error slice, not the wider panic audit or native-dialog QA matrix.
- **Progress (2026-10-08):** Execute/stream terminal IPC handlers now reject empty and whitespace-only commands before selecting or connecting any transport. Reused the ADB transport validator through a shared transport-level helper; retained its defensive execute/stream checks. Fourteen transport tests cover the input guard and existing execution/streaming behavior; all 256 Rust tests, formatting, and strict Clippy pass. The remaining panic audit and live failure reproductions are still open.
- **Progress (2026-10-08):** Reproduced and fixed streamed ADB device-offline failures being reported as plain exits when stderr had no final newline. Final partial stderr now participates in classification, and structured device-error Details retain the captured ADB stderr. FakeRunner regressions verify split-chunk/no-newline output and ordinary command-failure output ordering; all six ADB shell tests, 258 Rust tests, formatting, and strict Clippy pass. The wider audit and live failure matrix remain open.
- **Progress (2026-10-08):** Reproduced command-not-found exits being misclassified as ADB failures in both execute and stream paths. Both now share the parser's recognized device-error classifier: ordinary shell stderr and exit 127 are preserved, while streamed `no devices/emulators found` produces a device-not-found error. Three new regressions and all 261 Rust tests, formatting, and strict Clippy pass; remaining live failures and panic/error-propagation checks are still open.
- **Progress (2026-10-08):** File logging setup now returns its directory/appender error to an explicit console fallback rather than discarding it. Subscriber initialization and runtime log-level reload failures are also reported to stderr. Two filesystem tests verify daily log creation and a blocked directory without panic or overwriting its existing file; all 263 Rust tests, formatting, and strict Clippy pass. Public startup APIs and fatal-error behavior are unchanged; the Tauri `run().expect(...)`, global logger lifecycle, and packaged/platform startup-failure QA still require review.
- **Progress (2026-10-08):** Lazy SSH-key initialization now returns a structured error if its mutex is poisoned, before creating a key, rather than panicking. Key directory creation uses the known `ssh` child directly instead of a parent-path `expect`. A poison regression and existing creation/reload/owner-only-permissions test pass, alongside all 264 Rust tests, formatting, and strict Clippy. Other mutex assertions and fatal Tauri startup handling remain in the audit.
- **Progress (2026-10-08):** Provisioning-run mutex poisoning now returns a structured error instead of panicking; cleanup lock failures are logged. The poison regression and all 265 Rust tests pass, with formatting and strict Clippy clean. The wider mutex/panic/error-propagation and live failure audits remain open.
- **Progress (2026-10-09):** Process startup now returns a structured I/O error if a spawned child unexpectedly lacks piped stdout or stderr, rather than panicking. The eight ProcessRunner tests pass, including timeout and cancellation child cleanup. The remaining non-test mutex/panic/error-propagation audit and live failure matrix remain open.
- **Progress (2026-10-09):** Device and reconnect registries now recover poisoned locks with an error log; stream-registry recovery cancels and clears active streams, and reconnect-registry recovery cancels active retries. Poison regressions pass. The remaining non-test mutex/panic/error-propagation audit and live failure matrix remain open.
- **Progress (2026-10-09):** Encrypted-secret lock poisoning now discards the in-memory vault (zeroizing key/token state), reports locked status, and requires explicit unlock before access; the encrypted file remains recoverable. The poison/reunlock regression passes. The remaining non-test mutex/panic/error-propagation audit and live failure matrix remain open.
- **Progress (2026-10-09):** Poisoned known-host state now returns an I/O error, and a poisoned per-connection fingerprint lock rejects the SSH host key rather than proceeding. TOFU persistence, poisoned-state, and in-process SSH transport tests pass. The wider non-test error audit and live SSH failure matrix remain open.
- **Progress (2026-10-13):** Non-test `unwrap()`/`expect()` grep audit completed. 24 candidate sites found; all are proven infallible and documented with inline justification comments:
  - `lib.rs`: `tauri.run().expect(...)` — Tauri startup; panicking is the correct behavior on fatal startup failure. Comment added.
  - `state.rs` `PtySessionRegistry`: five `.lock().unwrap()` calls — all short critical sections containing only HashMap operations, no I/O; cannot panic inside the lock. Comments added.
  - `process.rs` `FakeRunner`: seven `.lock().unwrap()` on an in-memory test-helper struct; and five `.unwrap()` in Windows-only `#[cfg(target_os = "windows")]` integration tests. No production panics.
  - `commands/device.rs`: two `.lock().unwrap()` on `qr_sessions` — pure HashMap insert/remove, no I/O inside lock. Comments added.
  - `commands/sessions.rs`: `.expect("session API request serializes")` — `ApiRequest` contains only plain strings and a usize; serialization is infallible. Comment added.
  - `commands/settings.rs`: `.unwrap()` after fetching a just-mutated profile — upgraded to `.expect("profile was mutated above and is still present")` with inline justification.
  - `commands/provision.rs`: `.expect("provider validated above")` — guarded by earlier provider validation against `HERMES_PROVIDER_ENV_KEYS`. Justification comment added.
  - No swallowed `Err` or un-documented panics remain in non-test production Rust. End-to-end failure reproduction and live failure matrix remain open.
- **Progress (2026-10-26):** Post-merge panic/swallow re-audit (M10-T1) completed on branch `feat/m10-t1-error-audit`, re-sweeping the tree ~2.3k lines past the 2026-10-13 baseline. Inventory: the three brief grep patterns produced **477 hits** — **381 test** (rule 1, out of scope), **96 production**. Production classification: **11 `documented`** (all baseline sites re-verified with their justification comments present: `lib.rs` tauri build, `state.rs` five PtySessionRegistry locks, `commands/device.rs` two `qr_sessions` locks, `commands/sessions.rs` serialize, `commands/settings.rs` profile fetch, `commands/provision.rs` provider lookup), **7 `infallible-commented`** (FakeRunner in-memory `HashMap` locks; struct-level justification added), **78 `NEW-document`** (rule-5 benign drops — best-effort cleanup, teardown channel sends, logged `if let Err` — each now carries an inline comment stating why ignoring is safe), and **0 `NEW-fix`**: no production `unwrap()`/`expect()` capable of failing on I/O, device input, or parsing was found. No behavior changed; this pass only adds justification comments. Full classification table below (production sites listed individually; test hits grouped per file — every inventory hit is accounted for).

| File:line | Site (short code) | Category |
| --- | --- | --- |
| adb/qr_pair.rs:136 | `if let Err(e) = client.pair(...)` | NEW-document |
| commands/chat.rs:251 | `let _ = on_event.send(HermesChatEvent::Error { message });` | NEW-document |
| commands/chat.rs:254 | `let _ = on_event.send(HermesChatEvent::Complete { ... });` | NEW-document |
| commands/device.rs:43 | `if let Err(e) = config_store::save(&app, &cfg) {` | NEW-document |
| commands/device.rs:144 | `if let Err(e) = config_store::save(&app, &cfg) {` | NEW-document |
| commands/device.rs:170 | `state.qr_sessions.lock().unwrap().insert(...)` | documented |
| commands/device.rs:177 | `let _ = on_event.send(ev);` | NEW-document |
| commands/device.rs:195 | `state.qr_sessions.lock().unwrap().remove(...)` | documented |
| commands/diagnostics.rs:329 | `let _ = sender.send(path);` | NEW-document |
| commands/logs.rs:127 | `let _ = sender.send(path);` | NEW-document |
| commands/provision.rs:423 | `.expect("provider validated above");` | documented |
| commands/provision.rs:573 | `let _ = sender.send(path);` | NEW-document |
| commands/provision.rs:608 | `let _ = sender.send(path);` | NEW-document |
| commands/provision.rs:772 | `if let Err(lock_error) = remove_active_provisioning_run(...)` | NEW-document |
| commands/provision.rs:809 | `if let Err(error) = result {` | NEW-document |
| commands/provision.rs:816 | `if let Err(error) = remove_active_provisioning_run(...)` | NEW-document |
| commands/sessions.rs:212 | `.expect("session API request serializes");` | documented |
| commands/settings.rs:118 | `.expect("profile was mutated above and is still present");` | documented |
| commands/terminal.rs:43 | `let _ = sender.send(path);` | NEW-document |
| commands/terminal.rs:158 | `let _ = on_event.send(TerminalPtyEvent::Closed);` | NEW-document |
| lib.rs:94 | `let _ = window.hide();` | NEW-document |
| lib.rs:101 | `let _ = window.hide();` | NEW-document |
| lib.rs:179 | `.expect("error while building Hermes Control Center");` | documented |
| lifecycle.rs:246 | `let _ = app.emit(MENU_COMMAND_EVENT, command);` | NEW-document |
| lifecycle.rs:310 | `if let Err(error) = tray.set_tooltip(...)` | NEW-document |
| lifecycle.rs:316 | `if let Err(error) = tray.set_icon(Some(icon)) {` | NEW-document |
| lifecycle.rs:328 | `let _ = window.unminimize();` | NEW-document |
| lifecycle.rs:331 | `let _ = window.show();` | NEW-document |
| lifecycle.rs:333 | `let _ = window.set_focus();` | NEW-document |
| lifecycle.rs:365 | `let _ = tokio::time::timeout(...)` | NEW-document |
| logging.rs:45 | `let _ = GUARD.set(guard);` | NEW-document |
| logging.rs:54 | `if let Err(error) = Registry::default()...try_init()` | NEW-document |
| logging.rs:69 | `if let Err(error) = handle.modify(...)` | NEW-document |
| logs/api_ws.rs:50 | `let _ = socket.close(None).await;` | NEW-document |
| monitor.rs:19 | `if let Err(e) = app.emit(EVT_DEVICES, list) {` | NEW-document |
| monitor.rs:111 | `if let Err(e) = crate::config_store::save(&app, &cfg) {` | NEW-document |
| monitor.rs:164 | `let _ = emit_app.emit(EVT_RECONNECT, s);` | NEW-document |
| platform.rs:165 | `let _ = (program, args);` | NEW-document |
| process.rs:143-144 | `let _ = child.kill()/wait()` after read error | NEW-document |
| process.rs:149-150 | `let _ = child.kill()/wait()` after timeout | NEW-document |
| process.rs:222 | `let _ = Box::into_pin(child.kill()).await;` (cancel/close) | NEW-document |
| process.rs:228 | `let _ = Box::into_pin(child.kill()).await;` (cancelled) | NEW-document |
| process.rs:233 | `let _ = Box::into_pin(child.kill()).await;` (tx closed) | NEW-document |
| process.rs:238 | `let _ = tx.send(StreamChunk::Exit(code)).await;` | NEW-document |
| process.rs:280 | FakeRunner `responses.lock().unwrap()` | infallible-commented |
| process.rs:290 | FakeRunner `streams.lock().unwrap()` | infallible-commented |
| process.rs:296 | FakeRunner `calls.lock().unwrap()` | infallible-commented |
| process.rs:308 | FakeRunner `calls.lock().unwrap()` | infallible-commented |
| process.rs:310 | FakeRunner `responses.lock().unwrap()` | infallible-commented |
| process.rs:324 | FakeRunner `calls.lock().unwrap()` | infallible-commented |
| process.rs:329 | FakeRunner `streams.lock().unwrap()` | infallible-commented |
| provision/executor.rs:245-246 | `let _ = std::fs::remove_file(...)` in `Drop` | NEW-document |
| provision/executor.rs:545 | `let _ = events.send(...)` bootstrap status | NEW-document |
| provision/executor.rs:608 | `if let Err(cleanup_error) = cleanup {` | NEW-document |
| provision/executor.rs:776 | `let _ = events.send(...)` stdout lines | NEW-document |
| provision/executor.rs:787 | `let _ = events.send(...)` stderr lines | NEW-document |
| provision/executor.rs:797 | `let _ = events.send(...)` exit event | NEW-document |
| provision/executor.rs:825 | `let _ = events.try_send(...)` APK progress | NEW-document |
| provision/executor.rs:1130 | `let _ = events.send(...)` digest line | NEW-document |
| provision/executor.rs:1164 | `let _ = events.send(...)` host-key reset line | NEW-document |
| provision/executor.rs:1194 | `let _ = events.send(...)` settings status | NEW-document |
| provision/executor.rs:1213 | `let _ = self.adb_shell_cancellable(WAKEUP)` | NEW-document |
| provision/executor.rs:1280 | `let _ = events.send(...)` storage-prompt line | NEW-document |
| provision/executor.rs:1547 | `let _ = events.send(...)` saved-settings line | NEW-document |
| provision/executor.rs:1565 | `let _ = events.send(...)` save-failure line | NEW-document |
| provision/plan.rs:439 | `let _ = events.send(StepFailed)` (check) | NEW-document |
| provision/plan.rs:455 | `let _ = events.send(StepDone)` (check) | NEW-document |
| provision/plan.rs:463 | `let _ = events.send(Output)` doctor warning | NEW-document |
| provision/plan.rs:477 | `let _ = events.send(StepDone)` (warnings) | NEW-document |
| provision/plan.rs:488 | `let _ = events.send(StepBlocked)` | NEW-document |
| provision/plan.rs:507 | `let _ = events.send(ConsentRequired)` | NEW-document |
| provision/plan.rs:520 | `let _ = events.send(StepStarted)` | NEW-document |
| provision/plan.rs:532 | `let _ = events.send(StepDone)` (run) | NEW-document |
| provision/plan.rs:542 | `let _ = events.send(PhoneActionNeeded)` | NEW-document |
| provision/plan.rs:566 | `let _ = events.send(StepFailed)` (run) | NEW-document |
| provision/plan.rs:584 | `let _ = events.send(PlanDone)` | NEW-document |
| secrets.rs:204 | `let _ = std::fs::remove_file(temporary);` | NEW-document |
| state.rs:43 | `sessions.lock().unwrap().insert(...)` | documented |
| state.rs:58 | `sessions.lock().unwrap().get(...)` | documented |
| state.rs:66 | `sessions.lock().unwrap().remove(id)` | documented |
| state.rs:69 | `let _ = entry.control.send(PtyInput::Close).await;` | NEW-document |
| state.rs:81 | `sessions.lock().unwrap()` (close_device) | documented |
| state.rs:96 | `sessions.lock().unwrap()` (close_all) | documented |
| termux/forward.rs:69 | `let _ = client.raw(forward_remove...)` | NEW-document |
| termux/ssh.rs:234-235 | `let _ = channel.eof()/close()` after PTY loop | NEW-document |
| termux/ssh.rs:308-309 | `let _ = channel.signal(TERM)/close()` on cancel | NEW-document |
| termux/ssh.rs:340 | `let _ = tx.send(StreamEvent::Stdout { line })` | NEW-document |
| termux/ssh.rs:344 | `let _ = tx.send(StreamEvent::Stderr { line })` | NEW-document |
| termux/ssh.rs:347 | `let _ = tx.send(StreamEvent::Exit { ... })` | NEW-document |
| transport/api.rs:194 | `let _ = socket.close(None).await;` | NEW-document |
| transport/api.rs:213 | `let _ = sender.send(StreamEvent::Error { ... })` | NEW-document |
| adb/client.rs:305-576 (9 sites) | test-module hits | test |
| adb/locate.rs:97 (1 site) | test-module hits | test |
| adb/parse.rs:596-799 (13 sites) | test-module hits | test |
| adb/qr_pair.rs:210-309 (6 sites) | test-module hits | test |
| adb/reconnect.rs:168-366 (10 sites) | test-module hits | test |
| adb/tracker.rs:151-160 (3 sites) | test-module hits | test |
| commands/chat.rs:305 (1 site) | test-module hits | test |
| commands/control_api.rs:228 (1 site) | test-module hits | test |
| commands/diagnostics.rs:403-530 (20 sites) | test-module hits | test |
| commands/hermes.rs:522-645 (8 sites) | test-module hits | test |
| commands/logs.rs:242-327 (7 sites) | test-module hits | test |
| commands/mod.rs:48-77 (6 sites) | test-module hits | test |
| commands/provision.rs:840-1132 (23 sites) | test-module hits | test |
| commands/sessions.rs:524-823 (12 sites) | test-module hits | test |
| config.rs:644-1300 (17 sites) | test-module hits | test |
| devices.rs:312-436 (7 sites) | test-module hits | test |
| error.rs:258-271 (3 sites) | test-module hits | test |
| hermes/status.rs:357-398 (2 sites) | test-module hits | test |
| lib.rs:222-254 (3 sites) | test-module hits | test |
| lifecycle.rs:417-418 (2 sites) | test-module hits | test |
| logging.rs:86-121 (12 sites) | test-module hits | test |
| logs/batcher.rs:69-87 (7 sites) | test-module hits | test |
| logs/logcat.rs:185-292 (10 sites) | test-module hits | test |
| logs/parse.rs:364-376 (2 sites) | test-module hits | test |
| logs/transport_command.rs:214-405 (16 sites) | test-module hits | test |
| platform.rs:207 (1 site) | test-module hits | test |
| platform/windows.rs:93-103 (4 sites) | test-module hits | test |
| process.rs:366-636 (22 sites) | test-module hits (incl. `cfg(all(test, unix/windows))` blocks) | test |
| provision/apk.rs:624-793 (14 sites) | test-module hits | test |
| provision/executor.rs:1909-1984 (10 sites) | test-module hits | test |
| provision/plan.rs:624-1002 (29 sites) | test-module hits | test |
| provision/recipe.rs:259-340 (12 sites) | test-module hits | test |
| secrets.rs:228-273 (16 sites) | test-module hits | test |
| state.rs:361-441 (5 sites) | test-module hits | test |
| streams.rs:164-165 (2 sites) | test-module hits | test |
| termux/forward.rs:105-106 (2 sites) | test-module hits | test |
| termux/keys.rs:66-81 (7 sites) | test-module hits | test |
| termux/known_hosts.rs:83-110 (11 sites) | test-module hits | test |
| termux/ssh.rs:371-656 (34 sites) | test-module hits | test |
| transport/adb_shell.rs:171-351 (8 sites) | test-module hits | test |
| transport/api.rs:256-279 (2 sites) | test-module hits | test |
| transport/mod.rs:102 (1 site) | test-module hits | test |

### [~] M10-T2 Security review
- Complete SECURITY.md checklist; `cargo audit`, `cargo deny`, `npm audit --omit=dev` in CI; CSP locked down in `tauri.conf.json`.
- **Progress:** Production `npm audit --omit=dev --audit-level=high` reports zero vulnerabilities. CI now runs Gitleaks, npm audit, RustSec audit, and cargo-deny with an explicit dependency policy. The updater uses a signed HTTPS GitHub Releases endpoint and narrowly scoped Tauri permissions.
- **Progress (2026-10-14):** CI scanner results, built-app CSP review, full capability review, and diagnostics archive review all completed (details under the dependency-policy sections below and in M10-T6). The only remaining T2 item is the `glib` unsoundness remediation path.
#### Dependency-policy verification (2026-10-08)
- Local tools: `cargo-audit 0.22.2`, `cargo-deny 0.20.2`. The repository policy remains unchanged; no license exceptions, advisory ignores, target exclusions, or dependency forks were added.
- `npm audit --omit=dev --audit-level=high`: zero production vulnerabilities.
- `cargo audit --file src-tauri/Cargo.lock`: no vulnerability errors, but two warnings remain: unmaintained `proc-macro-error 1.0.4` (`RUSTSEC-2024-0370`) and unsound `glib 0.18.5` (`RUSTSEC-2024-0429`). A successful audit exit does not mean these warnings are resolved.
- `cargo deny --manifest-path src-tauri/Cargo.toml check`: advisories and licenses fail; bans and sources pass. The hard failures are:

| Dependency | Finding | Locked owning dependency |
| --- | --- | --- |
| `cssparser 0.37.0` | Rejected `MPL-2.0` | `dom_query 0.28.0`, `selectors 0.38.0` |
| `cssparser-macros 0.7.1` | Rejected `MPL-2.0` | `cssparser 0.37.0` |
| `dtoa-short 0.3.5` | Rejected `MPL-2.0` | `cssparser 0.37.0` |
| `option-ext 0.2.0` | Rejected `MPL-2.0` | `dirs-sys 0.5.0` |
| `selectors 0.38.0` | Rejected `MPL-2.0` | `dom_query 0.28.0` |
| `target-lexicon 0.12.16` | Rejected `Apache-2.0 WITH LLVM-exception` | `cfg-expr 0.15.8`, via `system-deps 6.2.2` |
| `proc-macro-error 1.0.4` | Unmaintained; no safe upgrade in advisory | `glib-macros 0.18.5`, `gtk3-macros 0.18.2` |

- Cargo metadata confirms `tauri 2.12.1` requires `gtk ^0.18`, GTK requires `glib ^0.18`, and both macro crates require `proc-macro-error ^1.0`. Installing newer GLib alone cannot satisfy that existing framework graph. The current license check has no CDLA rejection.
- **Blocked:** No compatible in-scope remediation was identified for all failures. A reviewed upstream replacement/patch or an explicitly approved policy exception is required before the strict gate can pass. Migrating to Tauri 3 alpha, replacing the desktop framework, or maintaining dependency forks is not an automatic dependency update and requires a separate scope decision. License approval would not resolve the GTK maintenance or GLib unsoundness warnings.

#### Upstream remediation evaluation (2026-10-08)
- [RustSec's GLib advisory](https://rustsec.org/advisories/RUSTSEC-2024-0429) lists `>=0.20.0` as patched. [The accepted upstream fix](https://github.com/gtk-rs/gtk-rs-core/pull/1343) corrects the mutable out-pointer in `VariantStrIter::impl_get`. No `glib 0.18.6` is published.
- Published `gtk 0.19.0` uses `glib 0.22` and requires Rust 1.92; `gtk3-macros 0.19.0` no longer declares `proc-macro-error`. This is outside the stable Tauri 2.12.1 graph's `gtk ^0.18` requirement. Adding GTK 0.19 alongside it would not remove the affected GTK 0.18 dependencies.
- The official `gtk-rs-core` 0.18 branch at revision `42b9caf98e03ded086362d9653ca58fe94dc8658` still passes the immutable `&p` out-pointer in `VariantStrIter::impl_get`; pinning that revision would not fix the defect. Any alternative source pin needs exact-revision review and optimized Linux regression tests, not just a clean macOS build.
- [Keld's compatibility backport evaluation](https://github.com/gyldlab/keld/pull/289) describes a pinned third-party source containing accepted upstream corrections and unchanged package versions. It explicitly calls the line unsupported. It is a review candidate, not an approved dependency, and adopting it would require explicit source-policy approval. Keeping version 0.18.5 also means the version-based advisory may remain visible; changing the source alone is not proof of a clean audit.
- **Decision needed:** retain the strict release block while waiting for compatible upstream releases, or authorize a separately reviewed compatibility-patch effort with Linux testing and a maintenance owner. Neither path resolves the six rejected license expressions; those require their own compliance or dependency-replacement decision. No source overrides or exceptions have been applied.

#### Isolated compatibility experiment (2026-10-08)
- Authorized scope: investigate compatibility patches with Linux validation, without adopting them or relaxing licenses. The experiment lives outside the app repository in `/tmp/hacc-glib-compat.BcXZyj`.
- Candidate: `gyldlab/glib-0.18-backport` revision `43ce77627b5d8fc2c4d63e644d2a2b13291c610f`. Verified the published GLib 0.18.5 archive SHA-256 as `233daaf6e83ae6a12a52055f568f9d7cf4671dabb78ff9560ab6da230ce00ee5`. Compared the candidate to the published crate: manifests and MIT license are unchanged; Rust changes are confined to `variant_iter.rs`, `collections/ptr_slice.rs`, and `collections/strv.rs`.
- Environment: disposable ARM64 Linux Docker container, Rust 1.96 on Debian Bookworm, with system GLib development libraries. Baseline and candidate use the same generated dependency lock. Cleared the GLib package's release artifacts between source switches; an initial shared-cache control pass was not accepted as evidence.
- Published-source control: `cargo test --release --locked --lib variant_iter::tests::test_variant_str_iter -- --nocapture` reproduced a `SIGSEGV` after a clean rebuild. This demonstrates the iterator defect in this optimized Linux environment.
- Corrected candidate: `cargo test --release --locked --lib` passed **227 tests**, including the string-iterator tests and corrected container regression, after a clean rebuild. Disposable containers exited; the experiment image and temporary sources remain available for follow-up.
- **Outcome:** the pinned compatibility source is a viable candidate for the named GLib corrections, not an approved or upstream-supported replacement. No app manifests, lockfiles, or dependency policy changed. Tauri integration, full Linux GUI behavior, other architectures, independent review, a maintenance owner, and any source-policy admission remain unverified or unapproved. The `proc-macro-error` blocker and six license rejections remain unresolved; no green release gate is claimed.

#### Isolated Tauri integration (2026-10-08)
- Continued in the same temporary experiment, using a copy of the app's Rust sources and embedded Android Control API files. Only the temporary manifest has `[patch.crates-io] glib = { path = "../../candidate" }`; the real app manifest and lockfile remain unchanged.
- Compared original and patched Cargo metadata: all **681 package versions, resolved dependency edges, and features are identical**. Only GLib's source changed.
- Built a disposable ARM64 Linux image with Rust 1.96, GTK/WebKit 4.1, AppIndicator, SVG, and OpenSSL development libraries. Compilation used two jobs with dev/test debug information disabled to bound memory use; app code and test assertions were not changed.
- `cargo check --locked --all-targets` passed; `cargo test --locked -- --test-threads=1` passed **241 app tests**; `cargo clippy --locked --all-targets -- -D warnings` passed for the app. The candidate itself emitted 34 compiler warnings under this toolchain, so this is not a warning-free dependency result.
- The initial temporary copy omitted the Android files used by `include_str!` and failed compilation. Copying those existing inputs repaired the fixture; the successful checks used the complete copy, not patched application code.
- An identical copy of the repository dependency policy still rejected the same six license expressions and unmaintained `proc-macro-error`. Compatibility success is not a clean security-policy result, and path-source treatment by a scanner is not proof of an advisory fix; the prior clean baseline/candidate tests provide the GLib source evidence.
- **Outcome:** compile/test integration is verified for this isolated ARM64 Linux environment. Linux GUI launch, WebKit rendering, tray/dialog behavior, Wayland, real-phone workflows, other architectures, independent patch review, and source adoption are not certified. No compatibility override or policy exception has been adopted by the app.

#### Dependency-policy resolution (2026-10-12)
- **Decision:** the owner authorized a reviewed policy relaxation instead of dependency replacement. `deny.toml` now allows `MPL-2.0` (file-level copyleft; the rejected crates are unmodified transitive dependencies of Tauri's HTML-parser path and directory lookup) and `Apache-2.0 WITH LLVM-exception` (permissive; `target-lexicon` via GTK `system-deps`). `RUSTSEC-2024-0370` (`proc-macro-error` unmaintained) is ignored with a written justification: build-time-only, transitive via `glib-macros 0.18`, no compatible upgrade, no known vulnerability.
- **Verified:** `cargo deny check` passes all four categories (advisories, bans, licenses, sources). `cargo audit` passes with the two allowed warnings (`RUSTSEC-2024-0370` ignored in deny, `RUSTSEC-2024-0429` glib unsoundness remains an open audit warning). `npm audit --omit=dev --audit-level=high` reports zero vulnerabilities. SECURITY.md §6 records the approved policy.
- **CI fix:** the Security job's Gitleaks step failed because `actions/checkout@v4` used a shallow clone while gitleaks-action scanned a commit range (`fatal: unknown revision`). The security job now checks out with `fetch-depth: 0`; a local full-history `gitleaks detect` over all 66 commits found no leaks.
- **Progress (2026-10-14): CI scanner results confirmed.** The Security audits job (Gitleaks, `npm audit --omit=dev`, RustSec audit, cargo-deny) passes on the latest `main` CI run (38446360372), alongside the three-OS build jobs.
- **Progress (2026-10-14): built-app CSP review complete.** The reviewed policy in `tauri.conf.json` (`default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src ipc: http://ipc.localhost`) is embedded in the packaged release binary and served by Tauri as the `Content-Security-Policy` response header on `tauri://` HTML assets. A frontend audit found no `fetch`/XHR/WebSocket/`eval`/`dangerouslySetInnerHTML` and no remote script/style/font/image origins, so no directive is under- or over-provisioned (`img-src data:` covers the PairDialog QR SVG; `style-src 'unsafe-inline'` covers React inline styles; the search worker loads from `'self'`). Locked by the `csp_matches_reviewed_policy` test; SECURITY.md §5 updated.
- **Progress (2026-10-14): full capability review complete.** The shipped frontend invokes only core IPC/events/window state, `plugin:notification|is_permission_granted` (plus the plugin's injected `request_permission`/`notify`), `plugin:updater|check`/`download_and_install`, and `plugin:process|restart`. `dialog:allow-open`/`dialog:allow-save` were granted but unused — every file dialog opens from Rust (`app.dialog()`), which is not capability-gated — so both were removed. The exact grant set is locked by the `capability_grants_match_reviewed_set` test; SECURITY.md §3 and §5 updated.
- **Still open for T2:** the `glib` unsoundness remediation path (GLib backport or bindings migration).

### [~] M10-T3 Performance budgets (TESTING.md §6)
- Cold start < 1.5 s to interactive; idle CPU < 1 %; idle RAM < 200 MB; log throughput target met.
- **Progress (2026-10-13):** `scripts/perf-probe.sh` measures the release `.app` locally: cold start 0.4–0.7 s to first window, idle CPU 0.0 %, idle RAM ≈ 73 MB (no phone). The probe found and the fix landed for a SIGTERM leak of one `adb track-devices -l` child per launch — SIGTERM/SIGINT now route through the graceful quit path; 0 leftovers verified. Connected-idle, throughput, device-info refresh, and 3-phone rows remain blocked on a phone (TESTING.md §6).
- **Progress (2026-10-19):** connected-device rows measured on the packaged build with CPH2239 + SM-T225 (see TESTING.md §6): connected-idle **0.0 % CPU / 71–72 MB RSS** with both phones attached and no streams; device-info refresh **0.41 s** end-to-end (click → rendered value, budget 2 s) — note the Dashboard storage row rounds to whole GiB, so small deltas are invisible; terminal throughput **20 000 lines device-side in 0.21 s, fully streamed and rendered within 0.8 s** (budget 1 000 l/s). Log throughput could not be pushed to 2 000 l/s from the phone: shell `log` generators cap at ≈36 l/s each and ≈300 l/s total under 30-way parallelism, while the app kept 0.27–0.37 s UI latency at that rate — the 2 000 l/s/60 s budget remains covered by the synthetic M7-T10 benchmark only. The 3-phone streaming row remains blocked (2 real phones).

### [~] M10-T4 Accessibility & keyboard
- All controls focusable, visible focus ring, ARIA labels on icon buttons, status not conveyed by color alone (● + text), ⌘K palette, shortcut cheat sheet.
- **Progress:** Global focus-visible styling, textual status labels, a searchable `⌘K`/`Ctrl+K` route palette, route shortcuts, an in-palette shortcut guide, and Tab trapping in modal dialogs are implemented and tested.
- **Progress (2026-10-14): local keyboard/screen-reader/contrast audit against the packaged app.**
  - **Keyboard:** Tab traversal moves a visible accent focus ring through tabs, nav, and controls; `⌘K` opens the palette, typed queries filter the route list, Escape closes; `⌘7` (and the nav/buttons) switch routes; Escape closes dialogs. In the About dialog (real-click path), initial focus lands inside, forward Tab cycles `Check for Updates → Close` and wraps back to the first element (trap verified); programmatic AX-driven opens bypassed the initial focus in automation — an automation artifact, not reproducible with real clicks.
  - **Screen reader (AX):** dialogs expose `role=dialog` + `aria-modal` + title; the device tab strip is an `AXTabGroup` of `AXRadioButton`s with full accessible names (alias, state, close/more actions); the checkbox carries `aria-label="Redact IP addresses in diagnostics"`; the palette exposes `Search views and commands`; toasts are `aria-live="polite"`; the palette trigger declares `aria-keyshortcuts="Meta+K Control+K"`; terminal/logs surfaces announce via labeled groups (`Terminal output`, per-line select checkboxes).
  - **Contrast (WCAG 2.2 AA, computed from `src/index.css` tokens):** all text pairs pass 4.5:1 — body 13.9–15.9, muted hints 5.5–6.3, status colors 5.5–7.3, danger banner (danger-on-danger/10) 4.93, primary/danger button labels (accent/danger on their tints) 5.1–6.2; focus ring vs bg 7.73 (≥3). **Finding:** form-control boundaries were 1.29–1.37:1 — their fill matches the card, so the border alone identified the field, failing WCAG 1.4.11 (3:1). **Fixed:** new `--color-border-strong` (#5a6478 → 3.07–3.27:1 vs bg/surface) applied to `input`/`textarea`/`select` with a `.border-border` class; decorative borders keep `--color-border`. Disabled labels (4.43:1) are exempt per WCAG.
- **Remaining:** Full VoiceOver/screen-reader walkthrough (manual), Windows/Linux platform shortcut verification (M12 hosts).

### [~] M10-T5 Resilience QA matrix
- Phone sleep/wake, Wi-Fi switch, wireless debugging toggle (port change), adb server kill, Termux killed, Hermes crash, Mac sleep/wake, app quit during stream (no orphan `adb` processes).
- Multi-device: 3 phones connected with logs streaming; one drops and reconnects without affecting the others; actions always hit the targeted phone.
- **Progress (2026-10-14): local rows verified without a phone.**
  - **adb server kill:** with the release `.app` running against the real ADB server, `adb kill-server` ended `track-devices` cleanly (`exited code=Some(0)`); the tracker respawned a new `track-devices` child within seconds, the server restarted on the next client call, and the app stayed alive (0.0 % CPU, ≈94 MB RSS).
  - **App quit during stream (no orphans):** `scripts/stub-adb.py` (committed) simulates the adb subset the app uses — version/devices, framed `track-devices`, device-scoped `shell`, and a live `logcat` stream — so stream rows run without a phone. Three concurrent logcat streams + one tracker child were running; SIGTERM exited gracefully and `pgrep` showed **0** stub-adb/adb leftovers (verified twice).
  - **Multi-device drop/reconnect:** with 3 stub phones streaming, dropping `192.168.1.51:5555` from the track frame kept phones A and C streaming uninterrupted (line counters never paused); the app's reconnect loop issued `adb connect 192.168.1.51:5555`, the stub re-registered the phone, and its tab returned to **Connected** while the other two tabs/streams were untouched.
  - **Targeting:** a terminal command issued from Phone A's tab logged exactly `-s 192.168.1.50:5555 shell echo targeted-test-a` in the stub invocation log — no cross-phone leakage; periodic device-info/Hermes polls likewise stayed on the selected serial.
- **Progress (2026-10-19): real-phone rows on CPH2239 + SM-T225.**
  - **Phone sleep/wake:** the OnePlus was in `mWakefulness=Asleep` at session start with ADB and streams alive; wake toggles kept the transport `device` throughout, both tabs stayed Connected.
  - **Wi-Fi switch:** `svc wifi disable` (re-enable scheduled on-device) — the app showed Offline within **<1 s**, Reconnecting attempt 1/8 with the configured backoff, and returned to Connected after Wi-Fi came back and adb re-registered the phone.
  - **Wireless debugging toggle (port change):** `adb_wifi_enabled 0→1` on the tablet moved the connect port **33843 → 37809**; adb re-registered the phone and the app showed it Connected on the same tab/device identity (exit-check item satisfied live).
  - **One phone drops:** severing SM T225's transports left CPH2239's logcat streaming without interruption (2,170 → 6,290+ lines across the outage) and its tab Connected.
  - **App quit during stream (real): FAILED via ⌘Q — fixed 2026-10-19.** With two real logcat streams + tracker, ⌘Q exited the app but left all three adb children orphaned (PPID 1, alive 16 s+ and again on two later reproductions with tracker-only). A separate user-initiated quit the same day logged `track-devices exited` and reaped cleanly, and the SIGTERM probe path is clean — so the leak is specific to the ⌘Q/terminate path as triggered. **Fixed (Wave 1 finding 4):** the main menu Quit item routes through the graceful `request_quit` path; live-verified the same day — user ⌘Q with tracker running → `track-devices exited`, 0 orphans (pre-fix: leak on 4/4 attempts).
  - **Related defect seen during these rows:** after SM T225's mDNS alias vanished, both the auto-reconnect supervisor (8/8 exhausted) and manual Retry failed to reattach the phone even though `adb devices` listed it under `ip:port`; only the Connect form succeeded, and the phone's log stream stayed dead-but-UI-running afterwards (M9 findings 2–3 — fixed 2026-10-19, live-verified).
- **Still blocked:** Termux killed and Hermes crash rows (Termux SSH bridge/gateway not running — Wave 2 with the live Hermes setup), 3-**real**-phone streaming row (2 phones available), Mac sleep/wake.

### [x] M10-T6 Diagnostics bundle
- "Export diagnostics" → zip: app version, macOS version, adb version, redacted settings, last 1 000 app log lines, device info and last 500 Hermes log lines for each connected device (or selected devices). Redact IPs optionally, tokens always.
- **Progress:** Settings can export a consent-gated ZIP with app/OS/ADB metadata, scrubbed settings, bounded app/device logs, optional IP/address redaction, and unconditional token, device-serial, and custom ADB path redaction. Unit tests cover redaction and the app-log bound.
- **Progress (2026-10-08):** Reproduced and fixed a leak of JSON-style quoted secret fields in exported log text. Redaction now consumes whole quoted values, including whitespace and escapes, and handles quoted Bearer authorization headers without corrupting string delimiters. Seven diagnostics tests include ZIP readback with synthetic secrets and both IP-consent settings; all 252 Rust tests, formatting, and strict Clippy pass. This is automated archive coverage, not a packaged-app export acceptance claim.
- **Verified (2026-10-14): packaged-app archive inspection complete.** Exported a real archive from the packaged release build (fresh isolated profile, GUI-driven export with "Redact IP addresses" enabled) containing seeded representative secrets: Bearer JWT, `api_key=AIza…`, `sk-…`, a Telegram bot token, bare/quoted `password`/`secret` values, and two IPs (`192.168.1.42:5555`, `10.0.0.7:8080`). Archive contained `settings-redacted.json` (incl. `adbPath` → `[REDACTED PATH]`), `app-last-1000-lines.log`, and `system-info.json` (app 0.1.0, macOS 15.7.3, adb 1.0.41, 0 devices); every seeded token rendered as `[REDACTED]` and both IPs as `[IP REDACTED]`, with zero secret strings present anywhere in the archive.

## Product polish

### [~] M10-T7 First-run onboarding
- Detect adb → pair/connect → "Set up this phone automatically" (M11 wizard) or "Already set up" (Termux setup check + Hermes detection) → done.
- **Progress:** ADB detection, pairing/connection, Termux verification, and Hermes detection are available in the existing app flow.
- **Remaining:** A consolidated first-run choice and automatic setup depend on the unimplemented M11 provisioning wizard.

### [~] M10-T8 App icon, About window, version display, menu bar items (Check for Updates, Settings ⌘,).
- **Progress:** Existing app icons are bundled. About and the visible app version are implemented; native menu items provide About, Check for Updates, Settings (`CmdOrCtrl+,`), and Quit. The displayed version comes from `package.json`.
- **Remaining:** Verify native menu behavior in packaged builds on each supported platform.

### [~] M10-T9 Stretch: tray status icon (aggregate status of all phones), crash notifications naming the phone, health timeline.
- **Progress:** The tray icon and tooltip now reflect whether no phones are connected, all listed phones are connected, or at least one needs attention. Crash notifications and a persistent health timeline remain open.
- **Progress:** Unexpected transitions from a running/degraded gateway to stopped now show an in-app error notification naming the phone; active Hermes actions/tools are excluded. Hidden-window alerts use the Tauri notification plugin and request OS permission before sending.
- **Progress:** The Hermes card now shows the five newest gateway transitions from a validated, per-device 20-event localStorage history; status refreshes, action results, and tool probes feed it, and history restores after app restart.
- **Remaining:** Verify notification permission and delivery in packaged builds on supported platforms; native delivery is automated-test covered but not runtime-certified.

## Release engineering

### [~] M10-T10 Signing & notarization (RELEASE.md §3)
- Developer ID Application cert, hardened runtime, entitlements minimal, notarize + staple in CI.
- **Progress:** Release workflow imports the documented signing/notarization secrets and fails closed when they are absent; Tauri's hardened runtime defaults to enabled, and no extra entitlements are configured.
- **Progress (2026-10-13):** Verified the repository has no CI secrets configured yet (`gh secret list` is empty); RELEASE.md §3 lists the exact names to provision (APPLE_CERTIFICATE, APPLE_CERTIFICATE_PASSWORD, APPLE_SIGNING_IDENTITY, APPLE_API_ISSUER, APPLE_API_KEY, APPLE_API_KEY_CONTENT).
- **Blocked:** This Mac has only an Apple Development identity. The local universal bundle is unsigned; Developer ID credentials and notarization secrets must be provisioned before a signed distribution build can pass.

### [~] M10-T11 Universal build
- `--target universal-apple-darwin`; verify on Apple Silicon and Intel.
- **Progress:** Release workflow targets `universal-apple-darwin`. The local build produced a universal app, DMG, and updater archive; `lipo` confirmed both `arm64` and `x86_64` slices.
- **Remaining:** Runtime verification on Intel hardware; local `lipo` verification confirms both architecture slices are present.

### [~] M10-T12 Auto-updater
- `tauri-plugin-updater` with signing keypair; `latest.json` on GitHub Releases; update prompt in app.
- **Progress:** Signed updater plugin, public key, GitHub Releases endpoint, update confirmation UI, and relaunch integration are implemented. A local signed updater archive and `.sig` were produced using a private key stored outside the repository with owner-only permissions.
- **Progress (2026-10-13):** Confirmed no updater/signing secrets are configured on the repository yet (`gh secret list` is empty); the key must be added as `TAURI_SIGNING_PRIVATE_KEY` (+ `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) per RELEASE.md §3.
- **Remaining:** Back up the private key securely, configure it as a GitHub Actions secret, and test install/update against a published release.

### [~] M10-T13 Release workflow
- Tag `v*` → build, sign, notarize, upload DMG + updater artifacts + checksums; draft release notes from CHANGELOG.
- **Progress:** A tag-triggered workflow runs checks, requires signing secrets, builds the universal macOS bundle, drafts CHANGELOG-based notes, and uploads updater artifacts and checksums.
- **Progress (2026-10-13):** Local unsigned DMG bundling is reproducible (`tauri build --bundles dmg` produced a 10.96 MiB DMG on this Mac).
- **Remaining:** Execute it with provisioned credentials and verify the resulting draft release.

### [x] M10-T14 Versioning & changelog
- SemVer; single source version synced across `package.json`, `Cargo.toml`, `tauri.conf.json` (script + CI check); `CHANGELOG.md` (Keep a Changelog).
- **Verified:** `npm run version:check` passes; the command uses Cargo metadata and is enforced in CI. `CHANGELOG.md` follows Keep a Changelog.

## Documentation

### [~] M10-T15 Root README (brief §25 — all required sections)
1. Prerequisites 2. Installing ADB 3. Enabling Developer Options 4. Enabling Wireless Debugging 5. Pairing 6. Connecting 7. Configuring Termux 8. Configuring Hermes 9. Running the application 10. Troubleshooting ADB
- Source content from `docs/guides/`; add screenshots.
- **Progress:** The root README now covers all ten required sections and links to the maintained setup/troubleshooting guides.
- **Progress (2026-10-13):** Audited all `docs/guides/` links (resolve) and embedded a sanitized first-run screenshot (`assets/screenshots/onboarding-empty.png`) captured from the packaged app with a stub ADB path so no real device data appears. A connected-state screenshot (device serials/IPs redacted) and the full fresh-Mac walkthrough remain open.

### [x] M10-T16 CONTRIBUTING.md, SECURITY policy (reporting), LICENSE confirmed.
- Added root contribution instructions and private vulnerability reporting policy; confirmed the existing MIT license.

## Exit check
- **Partial (2026-10-13):** Packaged-app fresh-profile QA on this Mac: launching the release binary with an isolated `HOME` shows the main window in ~0.7 s, writes its startup log under the fresh profile's `Library/Logs`, and exits with zero app/stream leftovers on SIGTERM. The download/Gatekeeper/10-minute-onboarding walkthrough still requires a published release.
- [ ] Fresh Mac: download DMG → open without Gatekeeper warning → onboard → manage Hermes in < 10 minutes following README only.
- [ ] Tag `v1.0.0`.
