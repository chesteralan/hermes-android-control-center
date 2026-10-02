// The only module that talks to Tauri. Everything else imports from here.
import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AdbInfo,
  AndroidDevice,
  AppConfig,
  DeviceInfo,
  ErrorPayload,
  MdnsService,
  QrPairEvent,
  QrSession,
  ReconnectStatus,
} from "../types";

export const EVENTS = {
  devices: "device://changed",
  reconnect: "device://reconnect",
} as const;

function isErrorPayload(e: unknown): e is ErrorPayload {
  return typeof e === "object" && e !== null && "kind" in e && "message" in e;
}

export function toErrorPayload(e: unknown): ErrorPayload {
  if (isErrorPayload(e)) return e;
  return { kind: "io", message: e instanceof Error ? e.message : String(e), details: null };
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toErrorPayload(e);
  }
}

export const ipc = {
  detectAdb: () => call<AdbInfo>("detect_adb"),
  getSettings: () => call<AppConfig>("get_settings"),
  updateSettings: (config: AppConfig) => call<AppConfig>("update_settings", { config }),
  listDevices: () => call<AndroidDevice[]>("list_devices"),
  connectDevice: (address: string) => call<AndroidDevice>("connect_device", { address }),
  disconnectDevice: (serial: string) => call<void>("disconnect_device", { serial }),
  retryConnection: (serial: string) => call<void>("retry_connection", { serial }),
  pairDevice: (address: string, code: string) => call<void>("pair_device", { address, code }),
  discoverDevices: () => call<MdnsService[]>("discover_devices"),
  getDeviceInfo: (serial: string) => call<DeviceInfo>("get_device_info", { serial }),
  startQrPairing: (onEvent: (e: QrPairEvent) => void) => {
    const channel = new Channel<QrPairEvent>();
    channel.onmessage = onEvent;
    return call<QrSession>("start_qr_pairing", { onEvent: channel });
  },
  cancelQrPairing: (sessionId: string) => call<void>("cancel_qr_pairing", { sessionId }),
  onDevicesChanged: (cb: (d: AndroidDevice[]) => void): Promise<UnlistenFn> =>
    listen<AndroidDevice[]>(EVENTS.devices, (e) => cb(e.payload)),
  onReconnect: (cb: (s: ReconnectStatus) => void): Promise<UnlistenFn> =>
    listen<ReconnectStatus>(EVENTS.reconnect, (e) => cb(e.payload)),
};

export type Ipc = typeof ipc;
