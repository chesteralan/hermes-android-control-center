import { create } from "zustand";
import { ipc } from "../lib/ipc";
import { notifyWhenWindowHidden } from "../lib/nativeNotifications";
import { useDevices } from "./devices";
import { useToasts } from "./toast";
import type {
  CommandResult,
  ComponentStatus,
  ErrorPayload,
  HermesAction,
  HermesActionResult,
  HermesInstallReport,
  HermesStatus,
  HermesTool,
} from "../types";

export interface HermesEntry {
  status: HermesStatus | null;
  healthHistory: HermesHealthEvent[];
  detection: HermesInstallReport | null;
  loadingStatus: boolean;
  detecting: boolean;
  action: HermesAction | null;
  result: HermesActionResult | null;
  tool: HermesTool | null;
  toolOutput: CommandResult | null;
  error: ErrorPayload | null;
  statusError: ErrorPayload | null;
}

export interface HermesHealthEvent {
  id: number;
  checkedAt: number;
  gateway: ComponentStatus;
}

const MAX_HEALTH_EVENTS = 20;
const HEALTH_HISTORY_PREFIX = "hacc:hermes:health-history:";

function healthHistoryStorageKey(deviceKey: string): string {
  return `${HEALTH_HISTORY_PREFIX}${encodeURIComponent(deviceKey)}`;
}

function isHealthEvent(value: unknown): value is HermesHealthEvent {
  return (
    typeof value === "object" &&
    value !== null &&
    "id" in value &&
    typeof value.id === "number" &&
    Number.isFinite(value.id) &&
    "checkedAt" in value &&
    typeof value.checkedAt === "number" &&
    Number.isFinite(value.checkedAt) &&
    "gateway" in value &&
    (value.gateway === "running" ||
      value.gateway === "degraded" ||
      value.gateway === "stopped" ||
      value.gateway === "unknown")
  );
}

function readHealthHistory(deviceKey: string): HermesHealthEvent[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const parsed: unknown = JSON.parse(
      localStorage.getItem(healthHistoryStorageKey(deviceKey)) ?? "[]",
    );
    return Array.isArray(parsed) ? parsed.filter(isHealthEvent).slice(-MAX_HEALTH_EVENTS) : [];
  } catch {
    return [];
  }
}

function persistHealthHistory(deviceKey: string, history: HermesHealthEvent[]): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(healthHistoryStorageKey(deviceKey), JSON.stringify(history));
  } catch {
    return;
  }
}

function recordGatewayStatus(
  deviceKey: string,
  entry: HermesEntry,
  status: HermesStatus,
): HermesHealthEvent[] {
  const previousGateway = entry.status?.gateway ?? entry.healthHistory.at(-1)?.gateway;
  if (previousGateway === status.gateway) return entry.healthHistory;
  const history = [
    ...entry.healthHistory,
    {
      id: (entry.healthHistory.at(-1)?.id ?? 0) + 1,
      checkedAt: status.checkedAt,
      gateway: status.gateway,
    },
  ].slice(-MAX_HEALTH_EVENTS);
  persistHealthHistory(deviceKey, history);
  return history;
}

export const EMPTY_HERMES: HermesEntry = {
  status: null,
  healthHistory: [],
  detection: null,
  loadingStatus: false,
  detecting: false,
  action: null,
  result: null,
  tool: null,
  toolOutput: null,
  error: null,
  statusError: null,
};

interface HermesStore {
  byDevice: Record<string, HermesEntry>;
  refresh: (key: string, serial: string) => Promise<void>;
  detect: (key: string, serial: string) => Promise<void>;
  runAction: (key: string, serial: string, action: HermesAction) => Promise<void>;
  runTool: (key: string, serial: string, tool: HermesTool) => Promise<void>;
  clearError: (key: string) => void;
}

export const useHermes = create<HermesStore>((set, get) => {
  const patch = (key: string, value: Partial<HermesEntry>) =>
    set((s) => ({
      byDevice: { ...s.byDevice, [key]: { ...(s.byDevice[key] ?? EMPTY_HERMES), ...value } },
    }));

  return {
    byDevice: {},
    refresh: async (key, serial) => {
      patch(key, { loadingStatus: true, statusError: null });
      try {
        const status = await ipc.getHermesStatus(serial);
        const current = get().byDevice[key] ?? EMPTY_HERMES;
        const previous = current.healthHistory.length
          ? current
          : { ...current, healthHistory: readHealthHistory(key) };
        const wasRunning =
          previous.status?.gateway === "running" || previous.status?.gateway === "degraded";
        patch(key, {
          status,
          loadingStatus: false,
          healthHistory: recordGatewayStatus(key, previous, status),
        });
        if (
          wasRunning &&
          status.gateway === "stopped" &&
          previous.action === null &&
          previous.tool === null
        ) {
          const device = useDevices.getState().devices[key];
          const deviceName = device?.model?.replace(/_/g, " ") || device?.deviceId || serial;
          const message = `Hermes gateway stopped unexpectedly on ${deviceName}`;
          void notifyWhenWindowHidden("Hermes gateway stopped", message).then((sentNative) => {
            if (!sentNative) useToasts.getState().push(message, "error");
          });
        }
      } catch (e) {
        patch(key, { loadingStatus: false, statusError: e as ErrorPayload });
      }
    },
    detect: async (key, serial) => {
      patch(key, { detecting: true, error: null });
      try {
        const detection = await ipc.detectHermes(serial);
        patch(key, { detection, detecting: false });
      } catch (e) {
        patch(key, { detecting: false, error: e as ErrorPayload });
      }
    },
    runAction: async (key, serial, action) => {
      patch(key, { action, result: null, tool: null, toolOutput: null, error: null });
      try {
        const result = await ipc.hermesAction(serial, action);
        const previous = get().byDevice[key] ?? EMPTY_HERMES;
        patch(key, {
          result,
          status: result.status,
          statusError: null,
          action: null,
          healthHistory: recordGatewayStatus(key, previous, result.status),
        });
      } catch (e) {
        patch(key, { action: null, error: e as ErrorPayload });
      }
    },
    runTool: async (key, serial, tool) => {
      patch(key, { tool, toolOutput: null, result: null, error: null });
      try {
        const toolOutput = await ipc.runHermesTool(serial, tool);
        patch(key, { tool: null, toolOutput });
        const status = await ipc.getHermesStatus(serial);
        const previous = get().byDevice[key] ?? EMPTY_HERMES;
        patch(key, {
          status,
          statusError: null,
          healthHistory: recordGatewayStatus(key, previous, status),
        });
      } catch (e) {
        patch(key, { tool: null, error: e as ErrorPayload });
      }
    },
    clearError: (key) => patch(key, { error: null, statusError: null }),
  };
});
