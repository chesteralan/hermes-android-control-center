# Release & Operations

## 1. Environments
| Channel | Trigger | Signing | Distribution |
|---|---|---|---|
| Dev | `npm run tauri dev` | none | local |
| CI build | PR | none (ad-hoc) | workflow artifact |
| Pre-release | tag `vX.Y.Z-alpha.N` / `-beta.N` | Developer ID + notarized (from M10) | GitHub pre-release |
| Stable | tag `vX.Y.Z` | Developer ID + notarized (macOS); Authenticode (Windows) and GPG (Linux) from v1.1 | GitHub Release + updater `latest.json` |

## 2. Versioning
- SemVer. Version kept identical in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` — `npm run version:check` enforced in CI.
- `CHANGELOG.md` (Keep a Changelog); every PR adds an entry under Unreleased.
- Settings schema `version` bumped with migration when config shape changes.

## 3. macOS signing & notarization
Prereqs: Apple Developer Program, "Developer ID Application" certificate.

CI secrets:
| Secret | Use |
|---|---|
| `APPLE_CERTIFICATE` | base64 .p12 |
| `APPLE_CERTIFICATE_PASSWORD` | .p12 password |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Name (TEAMID)` |
| `APPLE_API_ISSUER`, `APPLE_API_KEY`, `APPLE_API_KEY_CONTENT` | notarization via App Store Connect API key (preferred over Apple ID password); the workflow writes the `.p8` contents to a temporary file |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | updater signatures |

The GitHub release workflow accepts the App Store Connect `.p8` contents as
`APPLE_API_KEY_CONTENT` and writes it to a temporary runner file for
`APPLE_API_KEY_PATH`. Keep the updater private key out of the repository; the
configured public key is safe to commit. Back up the private key before
publishing, since losing it prevents future updates.

Build: `npm run tauri build -- --target universal-apple-darwin`. Tauri signs, notarizes and staples when the env vars are present. Verify:
```
codesign --verify --deep --strict --verbose=2 "Hermes Control Center.app"
spctl --assess --type execute --verbose "Hermes Control Center.app"
xcrun stapler validate "Hermes Control Center.dmg"
```

Not distributed via the Mac App Store (sandbox prevents spawning `adb`) — ADR-010.

### Windows & Linux (v1.1, M12)
| Platform | Artifacts | Signing | CI secrets |
|---|---|---|---|
| Windows | NSIS `.exe` (per-user), `.msi`, NSIS updater `.exe` + `.sig` | Authenticode PFX (OV/EV), SHA-256 and RFC 3161 timestamp | `WINDOWS_CERTIFICATE` (base64 PFX), `WINDOWS_CERTIFICATE_PASSWORD` |
| Linux | `.AppImage` + updater `.sig`, `.deb`, `.rpm` | Detached GPG signatures for `SHA256SUMS` and AppImage | `LINUX_GPG_KEY` (ASCII-armored private key), `LINUX_GPG_PASSPHRASE` |

Azure Trusted Signing is an alternative future signing provider, not configured by
this workflow. Hardware-bound/non-exportable certificates need a provider-specific
signing command instead of the PFX import. Publish the Linux public signing key and
its fingerprint through a trusted maintainer channel before asking users to verify
downloads. Never store signing secrets in source control. Secrets must be supplied
in GitHub repository/environment settings; no real signed Windows/Linux artifacts
have been produced during local M12 implementation.

Verify: `signtool verify /pa /v <installer>.exe` (Windows); `gpg --verify SHA256SUMS.asc` (Linux).

## 4. Release workflow (`.github/workflows/release.yml`)
1. Trigger on tag `v*`.
2. `npm ci`, full CI gates (TESTING.md §5).
3. Build matrix: universal macOS, Windows x86_64, Ubuntu 22.04 x86_64. Import macOS
	notarization credentials / Windows signing PFX; every build requires the updater key.
4. `tauri-apps/tauri-action` builds/signs bundles without publishing. Windows verifies
	timestamped installer signatures. Each job uploads uniquely platform-prefixed assets
	as workflow artifacts; no parallel job overwrites release metadata.
5. One publisher downloads all assets, generates `latest.json` with `darwin-universal`,
	`windows-x86_64`, `linux-x86_64`, and native Darwin aliases, then hashes the exact
	published filenames including updater signatures and metadata. Unbuilt Windows/Linux
	aarch64 targets are deliberately omitted. Missing assets/signatures fail the assembly.
6. Require `LINUX_GPG_KEY`, sign/verify `SHA256SUMS` and the AppImage, then upload all
	files to one draft release with CHANGELOG notes. A human publishes after QA.

Linux package installs do not invoke the AppImage updater. Maintainers distribute newer
`.deb`/`.rpm` packages separately; the app shows a package-manager notice. Release assembly
tests run with `npm run test:release`. The workflow is configured but Windows/Linux
runner execution, actual signing, and upgrade acceptance still require external evidence.

## 5. Release checklist
- [ ] All milestone tasks checked; CI green on tag commit
- [ ] Versions synced; CHANGELOG updated
- [ ] SECURITY.md §5 checklist done
- [ ] Manual QA matrix (M10-T5, including multi-device) on Apple Silicon + Intel, latest two macOS versions
- [ ] Fresh install test: DMG opens without Gatekeeper warning
- [ ] Update test: previous version auto-updates to new version
- [ ] Settings migration tested from previous version
- [ ] README screenshots current
- [ ] Release notes published

## 6. Support & operations
- Paths come from the Tauri path API; on macOS:
- App logs: `~/Library/Logs/<bundle-id>/` (tracing-appender, 7-day retention). Windows: `%LOCALAPPDATA%\<bundle-id>\logs`; Linux: `~/.local/share/<bundle-id>/logs`.
- Settings: `~/Library/Application Support/<bundle-id>/settings.json`. Windows: `%APPDATA%\<bundle-id>`; Linux: `~/.config/<bundle-id>`.
- Diagnostics bundle (Help → Export Diagnostics) is the standard bug-report attachment.
- Issue templates: bug (requires diagnostics bundle, macOS/Android/adb versions), feature request.
- Rollback: unpublish release + revert `latest.json` to previous version; settings migrations must be backward-tolerant for one minor version.
