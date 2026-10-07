# Spike: New-phone provisioning feasibility (M0-S4)

Date: 2026-10-05 · Target observed: OPPO CPH2239, Android 11 (SDK 30), arm64-v8a · Status: partial; fresh/reset-device checks remain open

## Safety boundary

The only connected phone is already provisioned and had a live Hermes chat running inside Debian/proot during this probe. It is not a fresh/reset test device. No APK was installed, no permission or power setting was changed, no bootstrap command was typed, and the phone was not rebooted. The phone was showing Android Settings when its foreground activity was checked.

## Release metadata

| Source | Termux release | Artifact / integrity information |
|---|---|---|
| GitHub Releases | Latest release API returned `v0.118.3`, published 2025-05-22 | `termux-app_v0.118.3+github-debug_arm64-v8a.apk` (35,106,607 bytes). The release's `+github-debug_sha256sums` lists SHA-256 `72fdb596045116bf5ba1b5bdf5b26fddb9acc0bd074ad9f2da9eb0ae85e83a4e`; a streamed download of the arm64 APK produced the same digest. |
| F-Droid | Package page lists `0.119.0-beta.3` (1022), Android 7+; F-Droid build metadata lists stable `0.118.3` (1002) | The package page links the signed repository APK, `.apk.asc` signature, source tarball, and build log. Use the signed F-Droid repository index as the artifact metadata authority; the page itself does not publish a SHA-256 value. |
| F-Droid Termux:Boot | `0.8.1` (1000), Android 5+ | The package page provides the APK, signature, source tarball, and build log. |
| GitHub Termux:Boot | Latest release shown as `v0.8.1` | Upstream documentation says add-ons must use the same signing source as Termux. Switching source requires uninstalling Termux and all its plugins, which deletes their app data. |

Sources: [Termux F-Droid package](https://f-droid.org/packages/com.termux/), [Termux GitHub releases](https://github.com/termux/termux-app/releases), [F-Droid Termux build metadata](https://gitlab.com/fdroid/fdroiddata/-/raw/master/metadata/com.termux.yml), [Termux:Boot F-Droid package](https://f-droid.org/en/packages/com.termux.boot/), [Termux:Boot installation instructions](https://github.com/termux/termux-boot#installation).

Do not attempt either source's APK as an in-place upgrade on the observed phone until its signing certificate is identified and a backup/reset test device is available. The observed version is newer than the current stable GitHub/F-Droid release, and the installer label `com.google.android.packageinstaller` does not identify which APK source signed it.

## Existing-device observations

- `adb devices -l` showed one online CPH2239 and one stale offline serial. Probes used only the online mDNS serial.
- Installed Termux reports version `0.119.0-beta.3`, version code 1022, target SDK 28, installer package `com.google.android.packageinstaller`; `com.termux.boot` is not installed.
- `READ_EXTERNAL_STORAGE` and `WRITE_EXTERNAL_STORAGE` are declared but not granted. `MANAGE_EXTERNAL_STORAGE` app-op is `default`. Android 11's `pm` does not implement `check-permission`; the package dump and app-op query were used instead.
- `/sdcard/Download` exists; `/sdcard/Download/hacc` does not. Shared-storage read/write is untested; SSH is now available, but Termux's special storage access is off.
- The user power whitelist did not include `com.termux`. Phantom-process settings queried as `null`, as expected for Android 11; Android 12-15 behavior remains untested.
- An existing ADB forward maps local `tcp:49438` to device `tcp:8022`; it was left unchanged. The local TCP listener accepted connections, but the remote side closed SSH handshakes, and no `sshd` process was visible. The active Hermes/proot process was left untouched.

## Installer and interaction notes

Static inspection of the current [official installer](https://hermes-agent.nousresearch.com/install.sh) shows stages for prerequisites, repository, Python environment/dependencies, config, products, setup, gateway, and completion. Its `setup` stage invokes `hermes setup` only when an interactive TTY is available; its gateway stage invokes `hermes gateway install --if-missing` (not `hermes gateway setup`). Both interactive stages open `/dev/tty`. The installer explicitly rejects running directly under Termux; the planned target is Debian inside proot. These are source observations, not a live installer run. Whether gateway installation attempts systemd and how the current installer edits PATH/profile files inside proot remain unverified.

The Termux RUN_COMMAND documentation requires both the sender's `com.termux.permission.RUN_COMMAND` permission and `allow-external-apps=true`; foreground command sessions may also require user interaction on Android 10+. A separate M0-S1 probe temporarily enabled `allow-external-apps=true` with a backup, then attempted only the no-op `true` command; Android denied the ADB-shell sender with `Requires permission com.termux.permission.RUN_COMMAND`. The original properties file was restored (mode 0600, setting unset) and the temporary ADB forward was removed. The M0-S4 keystroke path (`am start`, `input text`, Enter) was not attempted because it could inject text into the active Hermes session.

## Remaining tests

- On a fresh/reset phone, install matching-source Termux and Termux:Boot APKs; record OEM prompts and verify checksum/signature rejection behavior.
- Grant the required storage access with user consent, then create/read/remove a uniquely named probe under `/sdcard/Download/hacc/`.
- With a known empty Termux terminal, test `am start` followed by `input text` and Enter using a harmless marker command; verify completion without relying on a pre-existing session.
- Test the `dumpsys deviceidle whitelist` command and Android-version-specific phantom-process settings on Android 12, 13, 14, and 15, with reversible cleanup.
- Open Termux:Boot once, install a non-destructive boot marker, reboot, and verify it runs. Then test Hermes survival only on the reset test device.
- On the Debian/proot fixture, run the official installer with PTY capture; record prompts, PATH/profile changes, `hermes setup`, gateway service behavior, SIGUSR1/SIGTERM behavior, and `~/.hermes/gateway_state.json` and log paths.

Until those checks are performed on a spare/reset phone, M0-S4 remains partial. The earlier Termux/Hermes inventory is in [termux-access.md](termux-access.md).

## Live recheck (2026-10-06)

Wireless ADB and Termux SSH became available again. The app's ED25519 host-key pin matched the server key, and the saved app key authenticated as Termux uid `u0_a231`. Debian is still installed and Hermes v0.21.4 is present; no APK install, storage permission grant, shared-storage write, input injection, or reboot was performed. `allow-external-apps` remains unset and `/sdcard/Download` is not readable/writable from Termux, so provisioning still needs a reset/spare-device run.