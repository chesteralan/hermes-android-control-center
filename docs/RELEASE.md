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
| `APPLE_API_ISSUER`, `APPLE_API_KEY`, `APPLE_API_KEY_PATH` | notarization via App Store Connect API key (preferred over Apple ID password) |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | updater signatures |

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
| Windows | NSIS `.exe` (per-user), `.msi`, updater `.nsis.zip` + `.sig` | Authenticode (OV/EV cert or Azure Trusted Signing), timestamped | `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD` (or Azure signing credentials) |
| Linux | `.AppImage` (+ updater `.tar.gz` + `.sig`), `.deb`, `.rpm` | GPG-signed `SHA256SUMS` / AppImage signature | `LINUX_GPG_KEY`, `LINUX_GPG_PASSPHRASE` |

Verify: `signtool verify /pa /v <installer>.exe` (Windows); `gpg --verify SHA256SUMS.asc` (Linux).

## 4. Release workflow (`.github/workflows/release.yml`)
1. Trigger on tag `v*`.
2. `npm ci`, full CI gates (TESTING.md §5).
3. Import certificate into temporary keychain.
4. `tauri-apps/tauri-action` builds universal target, signs, notarizes, uploads `.dmg`, `.app.tar.gz`, `.sig`, `latest.json`.
5. Generate `SHA256SUMS`.
6. Draft release with CHANGELOG section; human publishes.

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
