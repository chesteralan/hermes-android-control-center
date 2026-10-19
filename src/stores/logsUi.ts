import { create } from "zustand";
import type { LogLine } from "../types";

export const LEVEL_FILTERS = ["info", "warn", "error", "debug", "unknown"] as const;
export type LevelFilter = (typeof LEVEL_FILTERS)[number];

export interface PauseSnapshot {
  deviceKey: string;
  lines: LogLine[];
  lastSequence: number;
}

/** Per-phone viewer state so switching device tabs never leaks search/pause. */
export interface LogsUiState {
  follow: boolean;
  pause: PauseSnapshot | null;
  searchInput: string;
  searchQuery: string;
  regexEnabled: boolean;
  caseSensitive: boolean;
  matchCursor: number;
  selectedLevels: Set<LevelFilter>;
}

export const DEFAULT_LOGS_UI: LogsUiState = {
  follow: true,
  pause: null,
  searchInput: "",
  searchQuery: "",
  regexEnabled: false,
  caseSensitive: false,
  matchCursor: 0,
  selectedLevels: new Set(LEVEL_FILTERS),
};

interface LogsUiStore {
  byDevice: Record<string, LogsUiState>;
  patch: (key: string, patch: Partial<LogsUiState>) => void;
}

/** Module-level store (not component state) so viewer settings survive
 *  navigating away from the Logs view and back. */
export const useLogsUi = create<LogsUiStore>((set) => ({
  byDevice: {},
  patch: (key, patch) =>
    set((s) => ({
      byDevice: { ...s.byDevice, [key]: { ...(s.byDevice[key] ?? DEFAULT_LOGS_UI), ...patch } },
    })),
}));
