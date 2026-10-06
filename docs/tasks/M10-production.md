# M10 — Hardening & Production (v1.0.0)

**Goal:** Ship a signed, notarized, documented, supportable 1.0. **Depends on:** M9, M11 (M8 may be marked optional for 1.0 if SSH transport is stable).

## Quality

### [~] M10-T1 Error audit
- Every brief §16 case reproduced and verified to show human message + Details: adb not installed, adb not found, device offline, unauthorized, connection refused, wireless debugging disabled, Termux unavailable, Hermes not found, command failed, network timeout.
- grep audit: no `unwrap()`/`expect()` in non-test Rust except proven-infallible with comment; no swallowed `Err`.
- **Progress:** A Rust payload regression test now checks that each listed failure category serializes a human-readable message and non-empty expandable details. End-to-end reproduction and the non-test Rust audit remain open.
- **Progress:** The Rust error-payload test covers the ten listed ADB/Termux/Hermes failure categories and asserts each serialized payload has a human message and expandable details. The non-test panic/error-propagation grep audit and end-to-end reproductions remain open.

### [ ] M10-T2 Security review
- Complete SECURITY.md checklist; `cargo audit`, `cargo deny`, `npm audit --omit=dev` in CI; CSP locked down in `tauri.conf.json`.

### [ ] M10-T3 Performance budgets (TESTING.md §6)
- Cold start < 1.5 s to interactive; idle CPU < 1 %; idle RAM < 200 MB; log throughput target met.

### [ ] M10-T4 Accessibility & keyboard
- All controls focusable, visible focus ring, ARIA labels on icon buttons, status not conveyed by color alone (● + text), ⌘K palette, shortcut cheat sheet.

### [ ] M10-T5 Resilience QA matrix
- Phone sleep/wake, Wi-Fi switch, wireless debugging toggle (port change), adb server kill, Termux killed, Hermes crash, Mac sleep/wake, app quit during stream (no orphan `adb` processes).
- Multi-device: 3 phones connected with logs streaming; one drops and reconnects without affecting the others; actions always hit the targeted phone.

### [ ] M10-T6 Diagnostics bundle
- "Export diagnostics" → zip: app version, macOS version, adb version, redacted settings, last 1 000 app log lines, device info and last 500 Hermes log lines for each connected device (or selected devices). Redact IPs optionally, tokens always.

## Product polish

### [ ] M10-T7 First-run onboarding
- Detect adb → pair/connect → "Set up this phone automatically" (M11 wizard) or "Already set up" (Termux setup check + Hermes detection) → done.

### [ ] M10-T8 App icon, About window, version display, menu bar items (Check for Updates, Settings ⌘,).

### [ ] M10-T9 Stretch: tray status icon (aggregate status of all phones), crash notifications naming the phone, health timeline.

## Release engineering

### [ ] M10-T10 Signing & notarization (RELEASE.md §3)
- Developer ID Application cert, hardened runtime, entitlements minimal, notarize + staple in CI.

### [ ] M10-T11 Universal build
- `--target universal-apple-darwin`; verify on Apple Silicon and Intel.

### [ ] M10-T12 Auto-updater
- `tauri-plugin-updater` with signing keypair; `latest.json` on GitHub Releases; update prompt in app.

### [ ] M10-T13 Release workflow
- Tag `v*` → build, sign, notarize, upload DMG + updater artifacts + checksums; draft release notes from CHANGELOG.

### [ ] M10-T14 Versioning & changelog
- SemVer; single source version synced across `package.json`, `Cargo.toml`, `tauri.conf.json` (script + CI check); `CHANGELOG.md` (Keep a Changelog).

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
