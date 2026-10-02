# Android & Termux Setup Guide

Source material for the root README (brief §25). Steps assume Android 11+ (Wireless Debugging).

## 1. Prerequisites
- macOS 13+ (Apple Silicon or Intel). Windows 10/11 and Linux (Ubuntu 22.04+, Fedora) from v1.1.
- Android phone, Android 11 or newer, on the same Wi-Fi as the computer
- Termux installed from **F-Droid or GitHub releases** (the Play Store build is outdated)
- Hermes Agent installed inside Termux (or let the app set it up)
- For development: Node.js LTS, Rust stable, plus Xcode Command Line Tools (macOS) / Visual Studio C++ Build Tools + WebView2 (Windows) / `libwebkit2gtk-4.1-dev` and build essentials (Linux)

## 2. Install ADB
**macOS**
```sh
brew install --cask android-platform-tools
adb version
```
Alternative: Android Studio SDK Manager → Platform-Tools (`~/Library/Android/sdk/platform-tools/adb`).

**Windows** (v1.1)
```powershell
winget install Google.PlatformTools
adb version
```
Alternatives: `scoop install adb`, `choco install adb`, or Android Studio (`%LOCALAPPDATA%\Android\Sdk\platform-tools\adb.exe`). Allow `adb` through Windows Firewall if prompted.

**Linux** (v1.1)
```sh
sudo apt install adb            # Debian/Ubuntu
sudo dnf install android-tools  # Fedora
sudo pacman -S android-tools    # Arch
```

The app auto-detects common locations on each OS; set a custom path in **Settings → ADB** otherwise.

## 3. Enable Developer Options
Settings → About phone → tap **Build number** 7 times → enter PIN. (Location of "Build number" varies by manufacturer, e.g. Software information on Samsung.)

## 4. Enable Wireless Debugging
Settings → System → Developer options → **Wireless debugging** → On → allow on the current network.

## 5. Pair the device (once per Mac)
If no phone is connected, the app shows the pairing options directly.

**Option A — QR code (easiest)**
1. In the app: **Pair with QR code** (or Device → Pair new device → QR code).
2. On the phone: Wireless debugging → **Pair device with QR code** → scan the code on the Mac screen.
3. The app finds the phone, pairs and connects automatically. Requires mDNS to work on your network.

**Option B — Pairing code (works when mDNS is blocked)**
1. On the phone: Wireless debugging → **Pair device with pairing code**. Note `IP:pairing-port` and the 6-digit code.
2. In the app: **Pair with code**, enter both. (CLI equivalent: `adb pair <ip>:<pairing-port> <code>`.)

The pairing port is **different** from the connection port.

## 6. Connect
1. On the Wireless debugging screen, note **IP address & Port**.
2. In the app: enter IP and Port → Connect, or press **Discover** to find it via mDNS. (CLI: `adb connect <ip>:<port>`.)
3. Accept any authorization prompt on the phone.

The port changes when wireless debugging is toggled or the phone reboots; the app tries to rediscover it automatically.

### Multiple phones
Repeat steps 3–7 for each phone (pair each one once). Every connected phone gets its own tab; the **Overview** shows all of them. In **Settings → Devices** give each phone an alias/color, enable auto-connect, and override Hermes commands if a phone's install differs from the defaults. Termux setup (§7) uses the same app public key on every phone.

## 7. Configure Termux

> **New phone?** After pairing (§5), use **Set up new phone** in the app. It installs Termux, applies battery settings, sets up SSH, installs proot-distro + your distro, installs and configures Hermes, and enables autostart — you only unlock the phone, accept any install prompt, and enter Hermes secrets. The manual steps below are for existing setups or troubleshooting.

`adb shell` cannot access Termux's files, so the app talks to Termux through SSH tunnelled over ADB. Nothing is exposed on Wi-Fi.

```sh
pkg update && pkg upgrade
pkg install openssh termux-services
mkdir -p ~/.ssh && chmod 700 ~/.ssh
# paste the public key shown in the app's Termux setup wizard:
echo "<public key from app>" >> ~/.ssh/authorized_keys
chmod 600 ~/.ssh/authorized_keys
# bind to localhost only
echo "ListenAddress 127.0.0.1" >> $PREFIX/etc/ssh/sshd_config
sv-enable sshd        # start now and on Termux launch (restart Termux once after installing termux-services)
termux-wake-lock      # keep Termux alive while the screen is off
```

Keep Termux running in the background:
- Exclude Termux from battery optimization (Settings → Apps → Termux → Battery → Unrestricted).
- Android 12+: the "phantom process killer" can kill background processes. Mitigate with Developer options → **Disable child process restrictions** (Android 14+) or `adb shell device_config set_sync_disabled_for_tests persistent; adb shell device_config put activity_manager max_phantom_processes 2147483647` (Android 12–13).

Verify in the app: Termux setup wizard → **Verify**.

## 8. Configure Hermes
In **Settings → Hermes**, set the commands for your installation. They are executed in Termux via the selected transport. Example shape (adjust to your install — the app does not assume paths):

| Setting | Example |
|---|---|
| Start command | `cd ~/hermes && nohup hermes gateway > ~/hermes.log 2>&1 &` |
| Stop command | `pkill -f "hermes gateway"` |
| Restart command | *(empty → stop then start)* |
| Status command | `hermes status` |
| Process match | `hermes gateway` |
| Log command | `tail -n 200 -F ~/hermes.log` |

Use **Hermes → Refresh** to confirm status is detected.

### Hermes inside proot-distro
Press **Settings → Hermes → Detect Hermes**: the app searches Termux and every installed proot-distro for the `hermes` command. Or set it manually:

| Setting | Value for Debian proot + official installer |
|---|---|
| Environment | proot-distro → `debian` |
| Start mode | **Supervised** (no systemd in proot; the app keeps `hermes gateway run` alive and relaunches it after `/restart`, `hermes update` or a watchdog exit) |
| Gateway command | `hermes gateway run` |
| Stop | handled by the supervisor (stop flag + SIGTERM) |
| Restart | graceful: SIGUSR1 (drains in-flight turns); "Restart now": SIGTERM |
| Status commands | `hermes gateway status`, `hermes status` |
| State file | `~/.hermes/gateway_state.json` |
| Process match | `hermes gateway run` |
| Logs | `~/.hermes/logs/gateway.log`, `~/.hermes/logs/tool_calls.log` |
| Version / Doctor / Update | `hermes --version` / `hermes doctor` / `hermes update` |

Write commands as you'd type them **inside** Debian; the app adds `proot-distro login debian -- …`. Use **Preview** to see the exact command.

**Installing Hermes manually inside Debian** (what the setup wizard does for you):
```sh
proot-distro install debian
proot-distro login debian
apt update && apt install -y curl ca-certificates git
curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash
source ~/.bashrc
hermes setup            # provider, model, keys
hermes gateway setup    # Telegram etc. — answer No to installing/starting a service
```
`hermes gateway install` / `start` / `stop` need systemd and won't work in proot; let the app supervise the gateway instead.

## 9. Run the application
```sh
npm install
npm run tauri dev      # development
npm run tauri build    # production bundle in src-tauri/target/release/bundle/
```
