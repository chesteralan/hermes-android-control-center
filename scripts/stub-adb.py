#!/usr/bin/env python3
"""Stub adb for local resilience QA (M10-T5) without a phone.

Simulates the adb subset the app uses: version detection, `devices -l`,
framed `track-devices -l`, device-scoped `shell` (identity + device-info
script), and a live `logcat` stream. Device membership is read from
$STUB_ADB_STATE/devices.txt on every track frame, so QA can drop or restore
a device mid-run; `adb connect <addr>` re-adds the address, which lets the
app's reconnect loop recover a dropped wireless device.

Every invocation is appended to $STUB_ADB_STATE/invocations.log with the
target serial, so tests can assert that actions hit the intended phone.

Usage: point the app's Settings > ADB path at this file (or seed
`adbPath` in settings.json), set STUB_ADB_STATE to a writable directory,
and manage devices.txt from the test driver. Not shipped in release
bundles; scripts only.
"""

import os
import signal
import sys
import time

STATE = os.environ.get("STUB_ADB_STATE", os.path.join(os.getenv("TMPDIR", "/tmp"), "hacc-stub-adb"))
DEVICES_FILE = os.path.join(STATE, "devices.txt")
INVOCATIONS = os.path.join(STATE, "invocations.log")
LINE_COUNTS = os.path.join(STATE, "logcat-counts")

DEVICE_TEMPLATES = {
    "192.168.1.50:5555": ("STUB0001", "Stub_Phone_A", "stub_phone_a"),
    "192.168.1.51:5555": ("STUB0002", "Stub_Phone_B", "stub_phone_b"),
    "192.168.1.52:5555": ("STUB0003", "Stub_Phone_C", "stub_phone_c"),
}


def ensure_state() -> None:
    os.makedirs(STATE, exist_ok=True)
    if not os.path.exists(DEVICES_FILE):
        open(DEVICES_FILE, "w").close()


def log_invocation(argv: list[str]) -> None:
    ensure_state()
    serial = argv[0] if argv and argv[0] == "-s" and len(argv) > 1 else "-"
    with open(INVOCATIONS, "a", encoding="utf-8") as fh:
        fh.write(f"{time.time():.3f} serial={serial} argv={' '.join(argv)}\n")


def read_devices() -> list[str]:
    ensure_state()
    with open(DEVICES_FILE, encoding="utf-8") as fh:
        return [line.rstrip("\n") for line in fh if line.strip()]


def device_line(serial: str) -> str:
    hw_id, model, device = DEVICE_TEMPLATES.get(serial, ("STUBDEV00", "Stub_Device", "stub_device"))
    return f"{serial}\tdevice product:stub model:{model} device:{device} transport_id:1"


def cmd_version(_: list[str]) -> int:
    print("Android Debug Bridge version 1.0.41")
    print("Version 37.0.1-14910828")
    return 0


def cmd_devices(_: list[str]) -> int:
    print("List of devices attached")
    for line in read_devices():
        print(line)
    print()
    return 0


def cmd_track_devices(_: list[str]) -> int:
    """Framed track-devices: 4 hex digits of payload length, then payload."""

    def emit(payload: str) -> None:
        data = payload.encode()
        sys.stdout.write(f"{len(data):04x}")
        sys.stdout.write(payload)
        sys.stdout.flush()

    last = None
    while True:
        current = "\n".join(read_devices())
        if current != last:
            emit(current + ("\n" if current else ""))
            last = current
        time.sleep(0.4)


def device_info_script(serial: str) -> int:
    hw_id, model, _ = DEVICE_TEMPLATES.get(serial, ("STUBDEV00", "Stub_Device", "stub_device"))
    print("@@props")
    print(f"[ro.serialno]: [{hw_id}]")
    print("[ro.boot.serialno]: [%s]" % hw_id)
    print("[ro.build.version.release]: [14]")
    print("[ro.build.version.sdk]: [34]")
    print("[ro.product.manufacturer]: [StubCo]")
    print(f"[ro.product.model]: [{model}]")
    print("[ro.product.brand]: [Stub]")
    print("@@battery")
    print("Current Battery Service state:")
    print("  level: 87")
    print("  scale: 100")
    print("  status: 3")
    print("@@df")
    print("Filesystem 1K-blocks Used Available Use% Mounted on")
    print("/dev/block/dm-5 10000000 4000000 6000000 40% /data")
    print("@@mem")
    print("MemTotal:        7864320 kB")
    print("MemFree:         3145728 kB")
    print("@@cpu")
    print("processor       : 0")
    print("Hardware        : Stub Platform")
    print("@@nproc")
    print("8")
    print("@@ip")
    print("    inet 192.168.1.50/24 brd 192.168.1.255 scope global wlan0")
    print("@@aid")
    print(hw_id.lower())
    print("@@termux")
    print("@@termuxver")
    print("@@end")
    return 0


def cmd_shell(serial: str, command: str) -> int:
    if "@@props" in command:
        return device_info_script(serial)
    if command.strip() == "getprop ro.serialno":
        print(DEVICE_TEMPLATES.get(serial, ("STUBDEV00",))[0])
        return 0
    # Any other shell probe (termux checks, hermes detection, …) reports a
    # quiet, successful no-op so the UI shows its "not installed/stopped"
    # empty states instead of an error.
    return 0


def cmd_logcat(serial: str, _: list[str]) -> int:
    ensure_state()
    count_path = os.path.join(LINE_COUNTS, serial.replace(":", "_"))
    os.makedirs(LINE_COUNTS, exist_ok=True)
    counter = 0
    if os.path.exists(count_path):
        try:
            counter = int(open(count_path, encoding="utf-8").read().strip() or 0)
        except ValueError:
            counter = 0

    def stop(_sig, _frame):
        sys.exit(0)

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    while True:
        counter += 1
        stamp = time.strftime("%m-%d %H:%M:%S")
        print(f"{stamp}.{counter % 1000:03d}  4242  4242 I StubLog({serial}): line {counter}")
        sys.stdout.flush()
        with open(count_path, "w", encoding="utf-8") as fh:
            fh.write(str(counter))
        time.sleep(0.1)


def cmd_connect(address: str) -> int:
    ensure_state()
    devices = read_devices()
    if any(line.split()[0] == address for line in devices if line.split()):
        print(f"already connected to {address}")
        return 0
    with open(DEVICES_FILE, "a", encoding="utf-8") as fh:
        fh.write(device_line(address) + "\n")
    print(f"connected to {address}")
    return 0


def main() -> int:
    argv = sys.argv[1:]
    if not argv:
        return 1
    log_invocation(argv)
    ensure_state()

    if argv[0] == "version":
        return cmd_version(argv)
    if argv[0] == "devices":
        return cmd_devices(argv)
    if argv[0] == "track-devices":
        return cmd_track_devices(argv)
    if argv[0] == "connect":
        return cmd_connect(argv[1] if len(argv) > 1 else "")
    if argv[0] in ("start-server", "kill-server", "disconnect"):
        return 0
    if argv[0] == "mdns":
        if len(argv) > 1 and argv[1] == "services":
            print("")
        return 0
    if argv[0] == "-s" and len(argv) >= 4:
        serial, kind = argv[1], argv[2]
        if kind == "shell":
            return cmd_shell(serial, " ".join(argv[3:]))
        if kind == "logcat":
            return cmd_logcat(serial, argv[3:])
    return 0


if __name__ == "__main__":
    sys.exit(main())
