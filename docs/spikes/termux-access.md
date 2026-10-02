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

## Findings over the SSH bridge (2026-10-02)

| # | Finding | Impact |
|---|---|---|
| 8 | Debian rootfs: `$PREFIX/var/lib/proot-distro/containers/debian/rootfs` (no `installed-rootfs`). `proot-distro list` → "Installed containers: * debian". | `guest_to_host_path` maps `/root/x` → `<container>/rootfs/root/x`. |
| 9 | `hermes` = `/root/.local/bin/hermes` → symlink to `/usr/local/lib/hermes-agent/venv/bin/hermes`; venv Python 3.13.5 (`pyvenv.cfg`). `HERMES_HOME=/root/.hermes`. | Version/Python readable from files without entering proot. |
| 10 | `proot-distro login debian -- …` costs ~4.4 s per call; reading files via the host path ~0.5 s. | Status reads host paths + `/proc`; only actions (start/stop/update) enter proot. |
| 11 | Termux (SSH session) **can** see proot guest processes: `/usr/local/lib/hermes-agent/venv/bin/python /root/.local/bin/hermes [args]`. ADB sees them too. | Process detection works from both; argv distinguishes gateway (`gateway run`) from interactive CLI (no args). |
| 12 | `gateway_state.json` said `running` (pid 14920) but was last updated 2026-09-25 — the process was gone. `gateway.log` ends with `CRITICAL gateway.shutdown_watchdog … exiting with code 75 so the service supervisor can restart it.` | Never trust the state file alone: verify PID is alive and heartbeat (`updated_at`) is fresh. Confirms the need for the ADR-015 supervisor. |
| 13 | Logs at `/root/.hermes/logs/{gateway,agent,errors}.log`, format `YYYY-MM-DD HH:MM:SS,mmm LEVEL logger: message`. | Matches `detect_level`; M7 parser gets the timestamp too. |
| 14 | `setsid` and `nohup` are present in Termux; `termux-services` running (`runsv sshd`). | Supervisor can use `nohup setsid`. |

## Still to verify
- SIGUSR1 drain / SIGTERM behaviour of `hermes gateway run` under proot.
- RUN_COMMAND intent from shell uid (M0-S1, writes to phone — run only with consent).
- Provisioning steps (M0-S4) — need a spare/reset phone.
