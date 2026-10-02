# Spike: Termux access & Hermes inventory (M0-S1 / M0-S2, read-only part)

Date: 2026-10-02 · Phone: OPPO CPH2239, Android 11 (SDK 30), arm64-v8a · adb 1.0.41 (37.0.1)

## Findings

| # | Finding | Impact |
|---|---|---|
| 1 | `adb shell` runs as `uid=2000(shell)` with group `readproc`. `ls /data/data/com.termux/files/home` → **Permission denied**. | Confirms ADR-004: Termux files/commands need SSH bridge. |
| 2 | `adb shell` **can** list all processes and read `/proc/<pid>/cmdline` of Termux-uid (`u0_a231`) processes; cannot read `environ`, `fd`, `cwd`. | Hermes **running** detection (PID, uptime, cmdline) works over plain ADB, before SSH is set up. Add `AdbProcessProbe` as first-tier status source. |
| 3 | Hermes process: `/usr/local/lib/hermes-agent/venv/bin/python /usr/local/lib/hermes-agent/hermes` (no extra argv), PPID = `bash -l` inside `proot`, which is a child of Termux's session `bash`. Running 3 days. | Guest argv **is visible** from Termux and ADB. Installer (as root in Debian) puts Hermes in `/usr/local/lib/hermes-agent`, not `~/.hermes`. Currently started in the foreground of a Termux session (no supervisor). Default `process_match` should be `hermes-agent/hermes`. Whether it is `gateway run` or interactive CLI isn't visible from argv — check over SSH. |
| 4 | proot-distro launch uses `--rootfs=.` with bind mounts under `$PREFIX/var/lib/proot-distro/containers/debian/…` (newer proot-distro layout), not `installed-rootfs/debian`. | `guest_to_host_path` and distro listing must support both `containers/<distro>` (new) and `installed-rootfs/<distro>` (old). Resolve over SSH via `proot-distro list` / directory probe. |
| 5 | Wireless device serial is the **mDNS form** `adb-<serialno>-<suffix>._adb-tls-connect._tcp` (adb auto-connected via mDNS), not `ip:port`. `adb mdns services` shows `adb-<serialno>-<suffix> _adb-tls-connect._tcp 192.168.x.x:port`. | Parsers and `is_wireless` must handle this form; `ip_address` resolved via mDNS service list or `ip addr`. `device_id` can be read directly from the serial (matches `ro.serialno`). |
| 6 | Termux package: `versionCode:1022`, `versionName=0.119.0-beta.3`, `installer=com.google.android.packageinstaller` (sideloaded GitHub build). | Installer detection: `com.android.vending` = Play Store (warn); `com.google.android.packageinstaller` / `org.fdroid.fdroid` / none = sideloaded/F-Droid. Version name with `-beta` → GitHub build. |
| 7 | Android 11 → phantom process killer not applicable (12+ only). | M11-T5 must skip per version. |

## Still to verify (needs SSH into Termux or a test phone)
- `hermes` argv for gateway vs CLI; `~/.hermes` location for root user in Debian (`/root/.hermes`).
- `nohup setsid` survival after SSH disconnect; SIGUSR1/SIGTERM behaviour in proot.
- RUN_COMMAND intent from shell uid (M0-S1, writes to phone — run only with consent).
- Provisioning steps (M0-S4) — need a spare/reset phone.
