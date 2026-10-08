// The only module that talks to Tauri. Everything else imports from here.
import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AdbInfo,
  AndroidDevice,
  AppConfig,
  CommandResult,
  ControlApiInstallPreview,
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
  ProvisionEvent,
  ProvisionPlan,
  ProvisionRecipe,
  ProvisionStepId,
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

export type LogExportFormat = "log" | "jsonl";
export type TerminalPtyEvent = { type: "data"; data: number[] } | { type: "closed" };
export type AppMenuCommand = "about" | "settings" | "checkForUpdates";

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
  getSecretStorageState: () =>
    call<"native" | "encryptedLocked" | "encryptedUnlocked">("get_secret_storage_state"),
  unlockSecretStorage: (passphrase: string, optedIn: boolean) =>
    call<void>("unlock_secret_storage", { passphrase, optedIn }),
  lockSecretStorage: () => call<void>("lock_secret_storage"),
  supportsInAppUpdates: () => call<boolean>("supports_in_app_updates"),
  detectAdb: () => call<AdbInfo>("detect_adb"),
  restartAdbServer: () => call<void>("restart_adb_server"),
  getSettings: () => call<AppConfig>("get_settings"),
  updateSettings: (config: AppConfig) => call<AppConfig>("update_settings", { config }),
  previewControlApiInstall: (serial: string) =>
    call<ControlApiInstallPreview>("preview_control_api_install", { serial }),
  installControlApi: (serial: string) => call<CommandResult>("install_control_api", { serial }),
  testControlApi: (serial: string) => call<string>("test_control_api", { serial }),
  listProvisionRecipes: () => call<ProvisionRecipe[]>("list_provision_recipes"),
  getProvisionRecipeSource: (recipeId: string) =>
    call<string>("get_provision_recipe_source", { recipeId }),
  saveProvisionRecipe: (source: string) =>
    call<ProvisionRecipe>("save_provision_recipe", { source }),
  importProvisionRecipe: () => call<ProvisionRecipe | null>("import_provision_recipe"),
  exportProvisionRecipe: (recipeId: string) =>
    call<string | null>("export_provision_recipe", { recipeId }),
  getProvisionPlan: (serial: string, recipeId: string) =>
    call<ProvisionPlan>("get_provision_plan", { serial, recipeId }),
  uninstallIncompatibleTermux: (serial: string, recipeId: string, confirmation: string) =>
    call<void>("uninstall_incompatible_termux", { serial, recipeId, confirmation }),
  resetProvisionProgress: (serial: string, recipeId: string) =>
    call<void>("reset_provision_progress", { serial, recipeId }),
  runProvision: (
    serial: string,
    recipeId: string,
    fromStep: ProvisionStepId | null,
    onlyStep: boolean,
    approvedSteps: ProvisionStepId[],
    onEvent: (event: ProvisionEvent) => void,
  ) => {
    const channel = new Channel<ProvisionEvent>();
    channel.onmessage = onEvent;
    return call<string>("run_provision", {
      serial,
      recipeId,
      fromStep,
      onlyStep,
      approvedSteps,
      onEvent: channel,
    });
  },
  cancelProvision: (runId: string) => call<boolean>("cancel_provision", { runId }),
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
  startTerminalPty: (
    serial: string,
    columns: number,
    rows: number,
    onEvent: (event: TerminalPtyEvent) => void,
  ) => {
    const channel = new Channel<TerminalPtyEvent>();
    channel.onmessage = onEvent;
    return call<string>("start_terminal_pty", { serial, columns, rows, onEvent: channel });
  },
  writeTerminalPty: (sessionId: string, data: number[]) =>
    call<void>("write_terminal_pty", { sessionId, data }),
  resizeTerminalPty: (sessionId: string, columns: number, rows: number) =>
    call<void>("resize_terminal_pty", { sessionId, columns, rows }),
  closeTerminalPty: (sessionId: string) => call<boolean>("close_terminal_pty", { sessionId }),
  exportTerminalText: (text: string) => call<string | null>("export_terminal_text", { text }),
  startLogStream: (serial: string, source: LogSourceKind, onBatch: (lines: LogLine[]) => void) => {
    const channel = new Channel<LogLine[]>();
    channel.onmessage = onBatch;
    return call<string>("start_log_stream", { serial, source, onBatch: channel });
  },
  exportLogs: (lines: LogLine[], format: LogExportFormat) =>
    call<string | null>("export_logs", { lines, format }),
  exportDiagnostics: (redactIps: boolean) =>
    call<string | null>("export_diagnostics", { redactIps }),
  onDevicesChanged: (cb: (d: AndroidDevice[]) => void): Promise<UnlistenFn> =>
    listen<AndroidDevice[]>(EVENTS.devices, (e) => cb(e.payload)),
  onReconnect: (cb: (s: ReconnectStatus) => void): Promise<UnlistenFn> =>
    listen<ReconnectStatus>(EVENTS.reconnect, (e) => cb(e.payload)),
  onMenuCommand: (cb: (command: AppMenuCommand) => void): Promise<UnlistenFn> =>
    listen<AppMenuCommand>("hacc://menu-command", (e) => cb(e.payload)),
};

export type Ipc = typeof ipc;
