# M10 — Hardening & Production (v1.0.0)

**Goal:** Ship a signed, notarized, documented, supportable 1.0. **Depends on:** M9, M11 (M8 may be marked optional for 1.0 if SSH transport is stable).

## Quality

### [~] M10-T1 Error audit
- Every brief §16 case reproduced and verified to show human message + Details: adb not installed, adb not found, device offline, unauthorized, connection refused, wireless debugging disabled, Termux unavailable, Hermes not found, command failed, network timeout.
- grep audit: no `unwrap()`/`expect()` in non-test Rust except proven-infallible with comment; no swallowed `Err`.
- **Progress:** A Rust payload regression test now checks that each listed failure category serializes a human-readable message and non-empty expandable details. Removed a guarded production `exit_code.unwrap()` from chat error formatting. End-to-end reproduction and the remaining non-test Rust panic/error-propagation audit remain open.
- **Progress:** The Rust error-payload test covers the ten listed ADB/Termux/Hermes failure categories and asserts each serialized payload has a human message and expandable details. The non-test panic/error-propagation grep audit and end-to-end reproductions remain open.

### [~] M10-T2 Security review
- Complete SECURITY.md checklist; `cargo audit`, `cargo deny`, `npm audit --omit=dev` in CI; CSP locked down in `tauri.conf.json`.
- **Progress:** Production `npm audit --omit=dev --audit-level=high` reports zero vulnerabilities. CI now runs Gitleaks, npm audit, RustSec audit, and cargo-deny with an explicit dependency policy. The updater uses a signed HTTPS GitHub Releases endpoint and narrowly scoped Tauri permissions.
- **Remaining:** CI scanner results, built-app CSP review, full capability review, and diagnostics archive review.

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
- **Remaining:** Inspect a real archive from a packaged app and verify redaction against representative logs before closing the ticket.

## Product polish

### [~] M10-T7 First-run onboarding
- Detect adb → pair/connect → "Set up this phone automatically" (M11 wizard) or "Already set up" (Termux setup check + Hermes detection) → done.
- **Progress:** ADB detection, pairing/connection, Termux verification, and Hermes detection are available in the existing app flow.
- **Remaining:** A consolidated first-run choice and automatic setup depend on the unimplemented M11 provisioning wizard.

### [~] M10-T8 App icon, About window, version display, menu bar items (Check for Updates, Settings ⌘,).
- **Progress:** Existing app icons are bundled. About and the visible app version are implemented; native menu items provide About, Check for Updates, Settings (`CmdOrCtrl+,`), and Quit. The displayed version comes from `package.json`.
- **Remaining:** Verify native menu behavior in packaged builds on each supported platform.

### [ ] M10-T9 Stretch: tray status icon (aggregate status of all phones), crash notifications naming the phone, health timeline.

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
