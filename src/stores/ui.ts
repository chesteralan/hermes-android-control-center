import { create } from "zustand";

export type PairTab = "qr" | "code";

interface UiStore {
  pairTab: PairTab | null;
  openPair: (tab: PairTab) => void;
  closePair: () => void;
}

export const useUi = create<UiStore>((set) => ({
  pairTab: null,
  openPair: (pairTab) => set({ pairTab }),
  closePair: () => set({ pairTab: null }),
}));
