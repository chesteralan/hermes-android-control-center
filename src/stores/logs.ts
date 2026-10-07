import { create } from "zustand";
import { ipc } from "../lib/ipc";
import type { ErrorPayload, LogLine, LogSourceKind } from "../types";

export const MAX_LOG_LINES = 20_000;

interface DeviceLogs {
  lines: LogLine[];
  source: LogSourceKind;
  streamId: string | null;
  starting: boolean;
  error: ErrorPayload | null;
}

const empty: DeviceLogs = {
  lines: [],
  source: "logcat",
  streamId: null,
  starting: false,
  error: null,
};

interface LogStore {
  /** Keyed by device key; each phone has its own ring buffer. */
  byDevice: Record<string, DeviceLogs>;
  append: (key: string, batch: LogLine[]) => void;
  start: (key: string, serial: string, source?: LogSourceKind) => Promise<void>;
  stop: (key: string) => Promise<void>;
  clear: (key: string) => void;
}

export const useLogs = create<LogStore>((set, get) => {
  const patch = (key: string, p: Partial<DeviceLogs>) =>
    set((s) => ({ byDevice: { ...s.byDevice, [key]: { ...(s.byDevice[key] ?? empty), ...p } } }));

  return {
    byDevice: {},

    append: (key, batch) =>
      set((s) => {
        const cur = s.byDevice[key] ?? empty;
        const merged = cur.lines.concat(batch);
        const lines =
          merged.length > MAX_LOG_LINES ? merged.slice(merged.length - MAX_LOG_LINES) : merged;
        return { byDevice: { ...s.byDevice, [key]: { ...cur, lines } } };
      }),

    start: async (key, serial, source = "logcat") => {
      const cur = get().byDevice[key];
      if (cur?.streamId || cur?.starting) return;
      patch(key, {
        starting: true,
        error: null,
        ...(cur?.source !== source ? { source, lines: [] } : {}),
      });
      try {
        const streamId = await ipc.startLogStream(serial, source, (batch) =>
          get().append(key, batch),
        );
        patch(key, { streamId, starting: false });
      } catch (e) {
        patch(key, { starting: false, error: e as ErrorPayload });
      }
    },

    stop: async (key) => {
      const id = get().byDevice[key]?.streamId;
      if (!id) return;
      await ipc.cancelStream(id);
      patch(key, { streamId: null });
    },

    clear: (key) => patch(key, { lines: [] }),
  };
});
