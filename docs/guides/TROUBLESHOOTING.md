# Troubleshooting

| Symptom (app message) | Likely cause | Fix |
|---|---|---|
| **ADB not found** | adb not installed, or app launched from Finder without your shell PATH | `brew install --cask android-platform-tools` (macOS), `winget install Google.PlatformTools` (Windows), `apt/dnf/pacman` (Linux); Settings → ADB → Detect, or set path manually |
| Windows: SmartScreen "unrecognized app" | New signing certificate without reputation | More info → Run anyway (verify publisher name) |
| Windows: connect times out | Windows Firewall blocked `adb.exe` | Allow `adb.exe` on private networks |
| Linux: "No secret service available" | No GNOME Keyring/KWallet running | Install/start a keyring, or opt in to the encrypted file fallback |
| Linux: blank or glitchy window | WebKitGTK GPU/driver issue | Launch with `WEBKIT_DISABLE_DMABUF_RENDERER=1` |
| **Device unauthorized** | RSA prompt not accepted | Unlock phone, accept "Allow debugging"; if no prompt: Developer options → Revoke USB debugging authorizations, re-pair |
| **Device offline** | Phone asleep, Wi-Fi changed, adb server stale | Wake phone; `adb kill-server` (app: Settings → ADB → Restart server); reconnect |
| **Connection refused** | Wrong port (it changed) or wireless debugging off | Re-open Wireless debugging screen, use the new port or press Discover |
| **Wireless debugging disabled** | Toggle turned off automatically (network change / reboot) | Re-enable; some phones disable it when leaving the trusted network |
| **Failed to authenticate / pairing failed** | Wrong/expired pairing code, or used connect port for pairing | Use the *pairing* port and a fresh code |
| **Network timeout** | Different subnet, client isolation on Wi-Fi, VPN | Same network, disable AP/client isolation, disconnect VPN on either side |
| **Discover finds nothing** | mDNS blocked on network | Connect manually with IP:port |
| **QR pairing stuck on "Waiting for phone"** / expired | mDNS blocked, phone on a different network, or QR not scanned within 2 min | Check both are on the same Wi-Fi, press Regenerate, or use **Pair with code** |
| Setup stuck at "Bootstrap SSH" | Phone locked, Termux not in front, or keystrokes went elsewhere | Unlock, open Termux, press **Retry step**; or run `sh /sdcard/Download/hacc/bootstrap.sh` in Termux yourself |
| Setup: `INSTALL_FAILED_UPDATE_INCOMPATIBLE` | Existing Termux from another source (signature differs) | Uninstall the old Termux (deletes its data) or switch the recipe source to match |
| Setup: "Allow install via USB/ADB" never appears | OEM restriction (e.g. Xiaomi "Install via USB" off) | Enable it in Developer options, retry step |
| Setup: some Android settings "not supported" | Android version / OEM differences | Optional — apply manually per §7 if Termux gets killed |
| **Termux unavailable** | Termux not running, sshd not started, killed in background | Open Termux, `sv up sshd`, `termux-wake-lock`; see ANDROID_SETUP §7 battery settings |
| **SSH host key mismatch** | Termux reinstalled or different device on same serial | Verify, then Settings → Termux → Forget host key |
| **Hermes not found** | Process match doesn't match running command line | `ps -ef | grep -i hermes` in Termux terminal; update Process match |
| **Hermes not installed** (but it is) | Installed inside proot-distro or not on PATH | Settings → Hermes → Detect Hermes, or set Environment to proot-distro + distro |
| Hermes stops when the app disconnects | Started in foreground / without `setsid` | Start mode → Supervised (or Detached); `pkg install util-linux` if `setsid` is missing |
| Gateway doesn't come back after `/restart`, `hermes update` or a crash | No service manager in proot | Start mode → **Supervised**; check `~/.hacc/supervisor.log` in Termux |
| `hermes gateway install` / `start` fails in Debian | systemd isn't available in proot | Expected — let the app supervise `hermes gateway run` |
| `hermes: command not found` from the app but works in Termux session | Installer added PATH only to `~/.bashrc` | Settings → Hermes → Detect Hermes (fills PATH automatically) |
| Status shows "heartbeat stale" / "degraded" | Gateway event loop stalled (`gateway_state.json`) | Restart; check `gateway.log`, `gateway_faulthandler.log`; run Doctor |
| Python version shown is wrong | Environment set to Termux while Hermes runs in proot | Set Environment to the proot distro |
| **Termux not installed** / Play Store warning | Termux missing or outdated Play Store build | Install from F-Droid or GitHub releases |
| **Command failed (exit N)** | Command error | Expand Details for stderr; run the same command in the Terminal |
| Commands work in Termux app but not in "Android shell" mode | `adb shell` runs as uid `shell`, not Termux | Switch terminal transport to **Termux** |
| Logs stop after a while | Termux killed / log file rotated | `tail -F` (capital F) in log command; check battery settings |
| `adb server version doesn't match this client` | Multiple adb installs (e.g. Homebrew + Android Studio) | Pick one path in Settings; `adb kill-server` |

## Useful CLI checks
```sh
adb devices -l
adb mdns services
adb -s <serial> shell getprop ro.build.version.release
adb -s <serial> forward --list
```

## Collecting diagnostics
Help → **Export Diagnostics** and attach the zip to the issue (tokens are always redacted; IPs optionally).
