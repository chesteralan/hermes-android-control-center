# Changelog

All notable changes to this project are documented here. This project follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Cross-platform ADB lookup/hints, Windows Job Object cancellation and private-file
  ACLs, native credential-store backends, and explicit passphrase-encrypted token storage.
- Platform modifier/monospace mappings, selection-aware terminal copy, Linux package
  updater notices, and a documented Windows/Linux runtime QA matrix.
- NSIS/MSI and AppImage/deb/rpm configurations with a three-OS signing workflow and
  one tested updater-manifest/checksum publisher. Actual signing and native platform
  install/update certification remain pending.
- CI check that keeps the application versions in package.json, Cargo.toml, and
  tauri.conf.json synchronized.

### Fixed

- Return a recoverable error for poisoned SSH-key initialization instead of
  panicking, and avoid a key-directory parent-path assertion.
- Report file-logging fallback, subscriber initialization, and log-level reload
  failures instead of silently discarding them; retain console logging fallback.
- Preserve ordinary shell command-not-found stderr and exit codes instead of
  treating them as ADB failures; classify missing-device errors consistently in streams.
- Classify streamed ADB device failures when stderr lacks a final newline and
  retain the captured ADB stderr in structured device-error details.
- Reject blank terminal commands before ADB/SSH/API setup so input errors are not
  obscured by connection or credential failures.
- Reject relative, empty, and root destinations consistently for log, terminal,
  and diagnostics exports; preserve native-dialog cancellation and write errors.
- Redact complete quoted JSON/log secret values and quoted Bearer headers in
  diagnostics exports, preserving string delimiters and escaped-value handling.
- Use certificate-verified native TLS for HTTP downloads and signed updates, removing
  embedded certificate-root dependencies rejected by the unchanged license policy.
- Enable feature-branch desktop CI, preserve binary fixtures during Windows checkout,
  target Rust audits at the actual crate, and isolate Windows PowerShell ACL operations
  from inherited PowerShell Core module paths.
- Remove optional SSH RSA support and the unpatched RSA timing-side-channel dependency;
  Ed25519 SSH execution, streaming, and cancellation remain tested.
