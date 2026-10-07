# Desktop Platforms

Cross-platform implementation is present, but Windows/Linux installation and runtime QA
have not yet been certified. No signed v1.1 release is claimed. Download only published
assets from [GitHub Releases](https://github.com/chesteralan/hermes-android-control-center/releases).

## Windows

Target QA environments: Windows 10 22H2 and Windows 11, x86_64. Install ADB with
`winget install Google.PlatformTools` or `scoop install adb`, or use Android Studio's
SDK Manager. Confirm `adb version` in a new terminal. Settings accepts an explicit
path to `adb.exe`; a configured path is authoritative.

For development, install Node.js LTS, Rust stable, Visual Studio 2022 Build Tools with
Desktop development with C++ and the Windows SDK, and the WebView2 runtime. Then run
`npm ci` and `npm run tauri dev` from a terminal with the build tools available.

Build installers with:

```powershell
npm run tauri build -- --config src-tauri/tauri.windows.conf.json
```

NSIS installs per-user without an administrator prompt. MSI is intended for managed
deployments and may require elevation under organizational policy. Both embed the
WebView2 bootstrapper; this downloads the runtime when absent and is not an offline
WebView2 installer. Release CI imports an Authenticode PFX certificate and uses SHA-256
with RFC 3161 timestamping. The NSIS executable is the signed updater asset; current
Tauri 2 does not require a legacy `.nsis.zip`.

Allow ADB through Windows Firewall only on trusted/private networks. Do not disable
SmartScreen: verify the signature and publisher before overriding a warning about a
new certificate's reputation. App-owned clients run without flashing consoles and in
Job Objects. The intentionally shared ADB server is started separately and is not
killed on app quit; test fresh-server startup and server restart during Windows QA.

SSH key files receive a protected ACL containing only the current user's SID. Native
Control API tokens use Windows Credential Manager.

## Linux

Target QA environments: Ubuntu 22.04/24.04 GNOME under Wayland and X11, and current
Fedora KDE. Install ADB using the distribution package manager:

```sh
# Debian / Ubuntu
sudo apt install adb
# Fedora
sudo dnf install android-tools
# Arch
sudo pacman -S android-tools
```

Android SDK platform-tools, `/snap/bin/adb`, and existing user/system Flatpak export
shims are also searched. The app does not install or grant a Flatpak sandbox escape.
Wireless ADB does not require USB udev rules; install distribution Android udev rules
only if using a USB connection outside this app's wireless provisioning workflow.

For development, install Node.js LTS, Rust stable, and these Debian/Ubuntu dependencies:

```sh
sudo apt install build-essential curl wget file libssl-dev libdbus-1-dev \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf
npm ci
npm run tauri dev
```

For Fedora, install the corresponding development packages with `dnf`: `gcc-c++`,
`openssl-devel`, `dbus-devel`, `webkit2gtk4.1-devel`, `libappindicator-gtk3-devel`,
`librsvg2-devel`, and `patchelf`. Distribution package names can change.

Build packages with:

```sh
npm run tauri build -- --config src-tauri/tauri.linux.conf.json
```

AppImage is the primary in-app-updatable bundle. `.deb`/`.rpm` installations show a
package-manager notice instead of invoking the AppImage updater; install newer
packages through `apt`, `dnf`, or the package installer. Release CI creates detached
GPG signatures for the AppImage and `SHA256SUMS`. Obtain the public signing key and
verify its fingerprint through a trusted maintainer channel before verification:

```sh
gpg --verify SHA256SUMS.asc SHA256SUMS
sha256sum -c SHA256SUMS
gpg --verify linux-x86_64-*.AppImage.asc linux-x86_64-*.AppImage
```

Some hosts need FUSE 2 compatibility to run an AppImage; extraction with
`--appimage-extract` is an alternative. GNOME/KDE file dialogs require a working
desktop portal (`xdg-desktop-portal` plus the desktop's backend). Clipboard and tray
support differ by desktop. Wayland uses normal close/minimize behavior when a tray
restore route is unavailable. For a blank/glitchy window, try:

```sh
WEBKIT_DISABLE_DMABUF_RENDERER=1 ./Hermes_Control_Center.AppImage
```

## Secret Storage

The SSH bridge uses Ed25519 client keys and supports non-RSA host keys such as
Termux's Ed25519/ECDSA keys. RSA support is disabled to remove the unpatched
`RUSTSEC-2023-0071` dependency; an RSA-only SSH server is not supported. If a phone
has only an RSA host key configured, enable an Ed25519 host key in Termux and verify
its fingerprint before forgetting the old pinned key in the app.

macOS uses Keychain, Windows uses Credential Manager, and Linux uses Secret Service.
Linux needs an unlocked session keyring such as GNOME Keyring or a Secret-Service-
enabled KWallet. Failure is reported clearly; the app never silently writes plaintext
tokens.

Minimal/headless desktop sessions may explicitly choose Settings > Secret storage >
Use passphrase-encrypted file storage. Use a unique passphrase of at least 12 characters.
Argon2id derives the key; ChaCha20-Poly1305 authenticates/encrypts the device-scoped
tokens. Only encrypted bytes are written, with Unix `0600` or a current-user Windows
ACL. The vault starts locked after every app restart; unlock before Control API use.
The passphrase is not part of settings, logs, or terminal history. Locking clears the
in-memory key; changing storage does not close already-active connections.

Existing system-store tokens are not automatically migrated. Reinstall the app-managed
Control API for each phone to place its new token in the unlocked encrypted vault.
Keep a backup of the encrypted file and its passphrase separately. Losing the passphrase
requires regenerating tokens. There is no passphrase recovery or automatic plaintext
fallback. To return to native storage, first stop Control API connections, back up and
remove `encrypted-secrets.json` from the app data directory, restart, then reinstall the
Control API to regenerate native-store tokens. Vault passphrase rotation is not yet a UI
operation.

## Certification

Use [Testing Strategy](../TESTING.md#7-cross-platform-qa-m12) for the required runtime,
resilience, performance, and signed-install/update checks. Unchecked items are not a
support claim. Signing credentials and actual Windows/Linux hosts are required to close
the milestone.