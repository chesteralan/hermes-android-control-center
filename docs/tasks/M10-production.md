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

### [~] M10-T2 Security review
- Complete SECURITY.md checklist; `cargo audit`, `cargo deny`, `npm audit --omit=dev` in CI; CSP locked down in `tauri.conf.json`.
- **Progress:** Production `npm audit --omit=dev --audit-level=high` reports zero vulnerabilities. CI now runs Gitleaks, npm audit, RustSec audit, and cargo-deny with an explicit dependency policy. The updater uses a signed HTTPS GitHub Releases endpoint and narrowly scoped Tauri permissions.
- **Remaining:** CI scanner results, built-app CSP review, full capability review, and diagnostics archive review.

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

### [ ] M10-T3 Performance budgets (TESTING.md §6)
- Cold start < 1.5 s to interactive; idle CPU < 1 %; idle RAM < 200 MB; log throughput target met.

### [~] M10-T4 Accessibility & keyboard
- All controls focusable, visible focus ring, ARIA labels on icon buttons, status not conveyed by color alone (● + text), ⌘K palette, shortcut cheat sheet.
- **Progress:** Global focus-visible styling, textual status labels, a searchable `⌘K`/`Ctrl+K` route palette, route shortcuts, an in-palette shortcut guide, and Tab trapping in modal dialogs are implemented and tested.
- **Remaining:** Full keyboard/screen-reader/contrast audit and platform shortcut verification.

### [ ] M10-T5 Resilience QA matrix
- Phone sleep/wake, Wi-Fi switch, wireless debugging toggle (port change), adb server kill, Termux killed, Hermes crash, Mac sleep/wake, app quit during stream (no orphan `adb` processes).
- Multi-device: 3 phones connected with logs streaming; one drops and reconnects without affecting the others; actions always hit the targeted phone.

### [~] M10-T6 Diagnostics bundle
- "Export diagnostics" → zip: app version, macOS version, adb version, redacted settings, last 1 000 app log lines, device info and last 500 Hermes log lines for each connected device (or selected devices). Redact IPs optionally, tokens always.
- **Progress:** Settings can export a consent-gated ZIP with app/OS/ADB metadata, scrubbed settings, bounded app/device logs, optional IP/address redaction, and unconditional token, device-serial, and custom ADB path redaction. Unit tests cover redaction and the app-log bound.
- **Progress (2026-10-08):** Reproduced and fixed a leak of JSON-style quoted secret fields in exported log text. Redaction now consumes whole quoted values, including whitespace and escapes, and handles quoted Bearer authorization headers without corrupting string delimiters. Seven diagnostics tests include ZIP readback with synthetic secrets and both IP-consent settings; all 252 Rust tests, formatting, and strict Clippy pass. This is automated archive coverage, not a packaged-app export acceptance claim.
- **Remaining:** Inspect a real archive from a packaged app and verify redaction against representative logs before closing the ticket.

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
- **Blocked:** This Mac has only an Apple Development identity. The local universal bundle is unsigned; Developer ID credentials and notarization secrets must be provisioned before a signed distribution build can pass.

### [~] M10-T11 Universal build
- `--target universal-apple-darwin`; verify on Apple Silicon and Intel.
- **Progress:** Release workflow targets `universal-apple-darwin`. The local build produced a universal app, DMG, and updater archive; `lipo` confirmed both `arm64` and `x86_64` slices.
- **Remaining:** Runtime verification on Intel hardware; local `lipo` verification confirms both architecture slices are present.

### [~] M10-T12 Auto-updater
- `tauri-plugin-updater` with signing keypair; `latest.json` on GitHub Releases; update prompt in app.
- **Progress:** Signed updater plugin, public key, GitHub Releases endpoint, update confirmation UI, and relaunch integration are implemented. A local signed updater archive and `.sig` were produced using a private key stored outside the repository with owner-only permissions.
- **Remaining:** Back up the private key securely, configure it as a GitHub Actions secret, and test install/update against a published release.

### [~] M10-T13 Release workflow
- Tag `v*` → build, sign, notarize, upload DMG + updater artifacts + checksums; draft release notes from CHANGELOG.
- **Progress:** A tag-triggered workflow runs checks, requires signing secrets, builds the universal macOS bundle, drafts CHANGELOG-based notes, and uploads updater artifacts and checksums.
- **Remaining:** Execute it with provisioned credentials and verify the resulting draft release.

### [x] M10-T14 Versioning & changelog
- SemVer; single source version synced across `package.json`, `Cargo.toml`, `tauri.conf.json` (script + CI check); `CHANGELOG.md` (Keep a Changelog).
- **Verified:** `npm run version:check` passes; the command uses Cargo metadata and is enforced in CI. `CHANGELOG.md` follows Keep a Changelog.

## Documentation

### [~] M10-T15 Root README (brief §25 — all required sections)
1. Prerequisites 2. Installing ADB 3. Enabling Developer Options 4. Enabling Wireless Debugging 5. Pairing 6. Connecting 7. Configuring Termux 8. Configuring Hermes 9. Running the application 10. Troubleshooting ADB
- Source content from `docs/guides/`; add screenshots.
- **Progress:** The root README now covers all ten required sections and links to the maintained setup/troubleshooting guides. UI screenshots and fresh-Mac walkthrough verification remain open.

### [x] M10-T16 CONTRIBUTING.md, SECURITY policy (reporting), LICENSE confirmed.
- Added root contribution instructions and private vulnerability reporting policy; confirmed the existing MIT license.

## Exit check
- [ ] Fresh Mac: download DMG → open without Gatekeeper warning → onboard → manage Hermes in < 10 minutes following README only.
- [ ] Tag `v1.0.0`.
