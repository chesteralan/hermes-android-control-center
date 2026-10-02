import { create } from "zustand";
import { ipc } from "../lib/ipc";
import type { AppConfig, ErrorPayload } from "../types";

interface SettingsStore {
  saved: AppConfig | null;
  draft: AppConfig | null;
  error: ErrorPayload | null;
  saving: boolean;
  load: () => Promise<void>;
  edit: (fn: (c: AppConfig) => AppConfig) => void;
  revert: () => void;
  save: () => Promise<boolean>;
}

export const useSettings = create<SettingsStore>((set, get) => ({
  saved: null,
  draft: null,
  error: null,
  saving: false,
  load: async () => {
    try {
      const cfg = await ipc.getSettings();
      set({ saved: cfg, draft: cfg, error: null });
    } catch (e) {
      set({ error: e as ErrorPayload });
    }
  },
  edit: (fn) => {
    const d = get().draft;
    if (d) set({ draft: fn(d) });
  },
  revert: () => set((s) => ({ draft: s.saved, error: null })),
  save: async () => {
    const draft = get().draft;
    if (!draft) return false;
    set({ saving: true, error: null });
    try {
      const saved = await ipc.updateSettings(draft);
      set({ saved, draft: saved, saving: false });
      return true;
    } catch (e) {
      set({ error: e as ErrorPayload, saving: false });
      return false;
    }
  },
}));

export const isDirty = (s: { saved: AppConfig | null; draft: AppConfig | null }) =>
  JSON.stringify(s.saved) !== JSON.stringify(s.draft);
