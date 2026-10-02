import { create } from "zustand";
import { ipc } from "../lib/ipc";
import type {
  CommandResult,
  ErrorPayload,
  HermesAction,
  HermesActionResult,
  HermesInstallReport,
  HermesStatus,
  HermesTool,
} from "../types";

export interface HermesEntry {
  status: HermesStatus | null;
  detection: HermesInstallReport | null;
  loadingStatus: boolean;
  detecting: boolean;
  action: HermesAction | null;
  result: HermesActionResult | null;
  tool: HermesTool | null;
  toolOutput: CommandResult | null;
  error: ErrorPayload | null;
}

export const EMPTY_HERMES: HermesEntry = {
  status: null,
  detection: null,
  loadingStatus: false,
  detecting: false,
  action: null,
  result: null,
  tool: null,
  toolOutput: null,
  error: null,
};

interface HermesStore {
  byDevice: Record<string, HermesEntry>;
  refresh: (key: string, serial: string) => Promise<void>;
  detect: (key: string, serial: string) => Promise<void>;
  runAction: (key: string, serial: string, action: HermesAction) => Promise<void>;
  runTool: (key: string, serial: string, tool: HermesTool) => Promise<void>;
  clearError: (key: string) => void;
}

export const useHermes = create<HermesStore>((set) => {
  const patch = (key: string, value: Partial<HermesEntry>) =>
    set((s) => ({ byDevice: { ...s.byDevice, [key]: { ...(s.byDevice[key] ?? EMPTY_HERMES), ...value } } }));

  return {
    byDevice: {},
    refresh: async (key, serial) => {
      patch(key, { loadingStatus: true, error: null });
      try {
        const status = await ipc.getHermesStatus(serial);
        patch(key, { status, loadingStatus: false });
      } catch (e) {
        patch(key, { loadingStatus: false, error: e as ErrorPayload });
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
        patch(key, { result, status: result.status, action: null });
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
        patch(key, { status });
      } catch (e) {
        patch(key, { tool: null, error: e as ErrorPayload });
      }
    },
    clearError: (key) => patch(key, { error: null }),
  };
});
