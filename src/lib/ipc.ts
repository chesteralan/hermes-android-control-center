// The only module that talks to Tauri. Everything else imports from here.
import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AdbInfo,
  AndroidDevice,
  AppConfig,
  CommandResult,
  DeviceInfo,
  ErrorPayload,
  HermesAction,
  HermesActionResult,
  HermesChatEvent,
  HermesInstallReport,
  HermesSessionPage,
  HermesSessionSummary,
  HermesStatus,
  HermesTool,
  LogLine,
  LogSourceKind,
  MdnsService,
  QrPairEvent,
  QrSession,
  ReconnectStatus,
  StreamEvent,
  TermuxCheck,
  TransportKind,
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
  executeCommand: (serial: string, command: string, transportKind: TransportKind = "adbShell") =>
    call<CommandResult>("execute_command", { serial, command, transportKind }),
  streamCommand: (
    serial: string,
    command: string,
    transportKind: TransportKind,
    onEvent: (e: StreamEvent) => void,
  ) => {
    const channel = new Channel<StreamEvent>();
    channel.onmessage = onEvent;
    return call<string>("stream_command", { serial, command, transportKind, onEvent: channel });
  },
  getTermuxPublicKey: () => call<string>("get_termux_public_key"),
  checkTermux: (serial: string) => call<TermuxCheck>("check_termux", { serial }),
  forgetTermuxHostKey: (serial: string) => call<void>("forget_termux_host_key", { serial }),
  getHermesStatus: (serial: string) => call<HermesStatus>("get_hermes_status", { serial }),
  detectHermes: (serial: string) => call<HermesInstallReport>("detect_hermes", { serial }),
  hermesAction: (serial: string, action: HermesAction) =>
    call<HermesActionResult>("hermes_action", { serial, action }),
  runHermesTool: (serial: string, tool: HermesTool) =>
    call<CommandResult>("run_hermes_tool", { serial, tool }),
  startHermesChat: (
    serial: string,
    prompt: string,
    sessionId: string | null,
    onEvent: (event: HermesChatEvent) => void,
  ) => {
    const channel = new Channel<HermesChatEvent>();
    channel.onmessage = onEvent;
    return call<string>("start_hermes_chat", { serial, prompt, sessionId, onEvent: channel });
  },
  listHermesSessions: (serial: string) =>
    call<HermesSessionSummary[]>("list_hermes_sessions", { serial }),
  getHermesSessionMessages: (serial: string, sessionId: string, offset = 0, limit = 500) =>
    call<HermesSessionPage>("get_hermes_session_messages", {
      serial,
      sessionId,
      offset,
      limit,
    }),
  cancelStream: (streamId: string) => call<boolean>("cancel_stream", { streamId }),
  startLogStream: (serial: string, source: LogSourceKind, onBatch: (lines: LogLine[]) => void) => {
    const channel = new Channel<LogLine[]>();
    channel.onmessage = onBatch;
    return call<string>("start_log_stream", { serial, source, onBatch: channel });
  },
  onDevicesChanged: (cb: (d: AndroidDevice[]) => void): Promise<UnlistenFn> =>
    listen<AndroidDevice[]>(EVENTS.devices, (e) => cb(e.payload)),
  onReconnect: (cb: (s: ReconnectStatus) => void): Promise<UnlistenFn> =>
    listen<ReconnectStatus>(EVENTS.reconnect, (e) => cb(e.payload)),
};

export type Ipc = typeof ipc;
