import type { AndroidDevice, DeviceInfo } from "../types";

export function device(over: Partial<AndroidDevice> = {}): AndroidDevice {
  return {
    serial: "adb-TESTSERIAL0001-a00nZY._adb-tls-connect._tcp",
    deviceId: "TESTSERIAL0001",
    model: "CPH2239",
    product: "CPH2239T2",
    device: "OP4F2F",
    transportId: "7",
    ipAddress: null,
    state: "device",
    rawState: "device",
    connection: "wirelessMdns",
    ...over,
  };
}

export function info(over: Partial<DeviceInfo> = {}): DeviceInfo {
  return {
    serial: device().serial,
    deviceId: "TESTSERIAL0001",
    manufacturer: "OPPO",
    model: "CPH2239",
    androidVersion: "11",
    sdk: 30,
    ipAddress: "192.0.2.25",
    battery: { level: 82, charging: true, status: "Charging", plugged: "AC", temperatureC: 31 },
    storage: { totalBytes: 100 * 1024 ** 3, freeBytes: 48 * 1024 ** 3 },
    memory: { totalBytes: 8 * 1024 ** 3, availableBytes: 4.8 * 1024 ** 3 },
    cpu: { abi: "arm64-v8a", cores: 8, hardware: "MT6765G" },
    termux: {
      installed: true,
      versionName: "0.119.0-beta.3",
      versionCode: 1022,
      installer: "com.google.android.packageinstaller",
      source: "sideloaded",
      companions: [],
    },
    ...over,
  };
}
