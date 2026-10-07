# Hermes Control Center

Desktop app (Tauri 2, Rust, and React) for managing a [Hermes Agent](https://github.com/NousResearch/hermes-agent) on an Android phone through Wireless ADB and Termux SSH. No screen mirroring is required.

**Release status:** pre-1.0. macOS is the first release target. Windows and Linux are planned for v1.1; see the [roadmap](docs/MILESTONES.md) and [open work](docs/TODO.md).

| Platform | Bundles | Verification Status |
|---|---|---|
| macOS | Universal app + DMG | Local Apple Silicon tests; signed-release/Intel QA pending |
| Windows x86_64 | Per-user NSIS + managed MSI | Build/signing workflow implemented; Windows runtime QA pending |
| Linux x86_64 | AppImage, `.deb`, `.rpm` | Build/signing workflow implemented; Wayland/X11 runtime QA pending |

Published downloads: [GitHub Releases](https://github.com/chesteralan/hermes-android-control-center/releases).
See [desktop setup, build prerequisites, and secret storage](docs/guides/DESKTOP_PLATFORMS.md).

## What the app does

- Discovers, pairs, connects, and reconnects to multiple Android devices.
- Shows device details including Android version, battery, storage, memory, CPU, and Termux status.
- Runs commands in Android shell or Termux, with streaming terminal sessions and command history.
- Manages Hermes status and gateway actions, and provides terminal and log views.
- Uses an SSH bridge forwarded over ADB. The Termux SSH service is configured for loopback only.

## 1. Prerequisites

- macOS 13 or newer for the current release target.
- An Android 11 or newer phone and a computer on the same Wi-Fi network.
- Termux from [F-Droid](https://f-droid.org/packages/com.termux/) or [GitHub releases](https://github.com/termux/termux-app/releases). The Play Store build is outdated.
- Hermes Agent installed in Termux or inside a configured `proot-distro` environment. The app can also guide new-phone setup.
- For development: Node.js LTS, Rust stable, and Xcode Command Line Tools.

## 2. Install ADB

On macOS, install Android Platform-Tools:

```sh
brew install --cask android-platform-tools
adb version
```

Alternatively, install Platform-Tools through Android Studio SDK Manager. The app detects common ADB locations; set a custom executable under **Settings > ADB** if it is installed elsewhere.

Windows and Linux packaging configuration is implemented for v1.1; signing and runtime QA
remain pending. See [desktop platforms](docs/guides/DESKTOP_PLATFORMS.md) for package-manager
commands and development prerequisites.

## 3. Enable Android Developer Options

On the phone, open **Settings > About phone** and tap **Build number** seven times. Enter the device PIN if prompted. The exact menu location varies by manufacturer; on some phones, Build number is under **Software information**.

## 4. Enable Wireless Debugging

Open **Settings > System > Developer options > Wireless debugging**, turn it on, and allow debugging on the current Wi-Fi network. Keep this screen available for pairing and connection details.

## 5. Pair the Android phone

Pair once per computer.

**QR code:** In the app choose **Pair with QR code**. On the phone, open **Wireless debugging > Pair device with QR code** and scan the code. This requires mDNS discovery on the network.

**Pairing code:** On the phone choose **Pair device with pairing code**. In the app choose **Pair with code** and enter the displayed IP address, pairing port, and six-digit code. The pairing port differs from the connection port. This method works when mDNS is blocked.

## 6. Connect the phone

On the Wireless debugging screen, use **IP address & Port** in the app's Device view, or choose **Discover** to find the device through mDNS. Accept the debugging authorization prompt on the phone.

The connection port may change after a reboot or when Wireless debugging is toggled. The app attempts to rediscover the phone; use **Retry** or enter the new port if needed. Repeat pairing and connection for each phone.

## 7. Configure Termux

The app cannot access Termux files through `adb shell`; it uses SSH through an ADB forward instead. The SSH daemon should listen on `127.0.0.1`, not on the phone's Wi-Fi interface.

In Termux, install and enable the SSH service:

```sh
pkg update && pkg upgrade
pkg install openssh termux-services
mkdir -p ~/.ssh && chmod 700 ~/.ssh
```

In the app, open **Termux setup** and copy its public key into `~/.ssh/authorized_keys`, then run:

```sh
chmod 600 ~/.ssh/authorized_keys
echo "ListenAddress 127.0.0.1" >> "$PREFIX/etc/ssh/sshd_config"
sv-enable sshd
termux-wake-lock
```

If `termux-services` was just installed, fully exit and reopen Termux before enabling `sshd`. In the app, choose **Verify** in the Termux setup view. Exclude Termux from battery optimization if Android stops it in the background.

For a new phone, use **Set up new phone** in the app when available. Detailed setup and Android-version notes are in the [Android setup guide](docs/guides/ANDROID_SETUP.md#7-configure-termux).

## 8. Configure Hermes

Open **Settings > Hermes** and choose **Detect Hermes** or configure the environment and commands for the installation. Use **Hermes > Refresh** to check detection.

For Hermes inside Debian `proot-distro`, select **proot-distro > debian** and use **Supervised** start mode with `hermes gateway run`. The app supervises the foreground gateway because systemd is not available inside proot. Run `hermes setup` and configure provider credentials in the selected Hermes environment.

Do not copy commands blindly from another phone: process paths, log locations, and Termux versus proot environments can differ. See [Hermes configuration](docs/guides/ANDROID_SETUP.md#8-configure-hermes) for the Debian example and command reference.

## 9. Run the application

For a development build:

```sh
npm install
npm run tauri dev
```

To create a local production bundle:

```sh
npm run tauri build
```

The bundle is written under `src-tauri/target/release/bundle/`. For contributor checks, run `npm test -- --run`, `npm run test:rust`, `npm run lint`, and `npm run typecheck`.

## 10. Tray and window behavior

On macOS, Windows, and Linux/X11, closing or minimizing the main window hides it to the Hermes Control Center tray icon when tray support initializes successfully. Choose **Open Hermes Control Center** from the tray menu to restore the existing window; active device connections and streams continue while it is hidden. Choose **Quit Hermes Control Center** to cancel streams, close interactive sessions, and release managed ADB/SSH resources.

Some Linux Wayland desktops do not provide a detectable tray host. In that case, and whenever tray initialization fails, the app keeps normal minimize behavior and closing the window exits the app. The window is never hidden without a working tray restore route.

Use **About > Check for Updates** to check for a signed release. Review the
version and release notes before installing; the app restarts after installation.
Linux `.deb`/`.rpm` installs use their package manager instead of the AppImage updater.

## 11. Troubleshoot ADB connections

| Symptom                | Likely fix                                                                                                                                 |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| ADB not found          | Install Platform-Tools, then use **Settings > ADB > Detect** or select the executable path.                                                |
| Device unauthorized    | Unlock the phone and accept the debugging authorization prompt. If it does not return, revoke USB debugging authorizations and pair again. |
| Device offline         | Wake the phone, confirm both devices are on the same network, then reconnect. Restart the ADB server from Settings if needed.              |
| Connection refused     | Reopen Wireless debugging and use its current **IP address & Port**; the port changes after toggling debugging or rebooting.               |
| Pairing fails          | Generate a fresh pairing code and use the pairing port, not the connection port.                                                           |
| Discover finds nothing | mDNS may be blocked. Pair with a code or connect using the phone's IP address and connection port.                                         |
| Termux unavailable     | Open Termux, check that `sshd` is running, and use the Termux setup view's **Verify** action.                                              |

See the full [troubleshooting guide](docs/guides/TROUBLESHOOTING.md) for Termux, Hermes, Windows, and Linux issues. The [documentation index](docs/README.md) links to architecture, security, testing, and release information. Contributors can start with [CONTRIBUTING.md](CONTRIBUTING.md); vulnerability reporting is in [SECURITY.md](SECURITY.md).
