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
