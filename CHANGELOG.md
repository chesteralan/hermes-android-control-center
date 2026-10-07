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

- Use certificate-verified native TLS for HTTP downloads and signed updates, removing
  embedded certificate-root dependencies rejected by the unchanged license policy.
- Enable feature-branch desktop CI, preserve binary fixtures during Windows checkout,
  target Rust audits at the actual crate, and isolate Windows PowerShell ACL operations
  from inherited PowerShell Core module paths.
- Remove optional SSH RSA support and the unpatched RSA timing-side-channel dependency;
  Ed25519 SSH execution, streaming, and cancellation remain tested.
