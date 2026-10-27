import { create } from "zustand";

export type PairTab = "qr" | "code";

/** localStorage key marking that the first-run setup guide was seen. */
export const ONBOARDING_DISMISS_STORAGE = "hacc.onboarding.dismissed";

export function loadOnboardingDismissed(): boolean {
  try {
    return localStorage.getItem(ONBOARDING_DISMISS_STORAGE) === "1";
  } catch {
    return false;
  }
}

function persistOnboardingDismissed(): void {
  try {
    localStorage.setItem(ONBOARDING_DISMISS_STORAGE, "1");
  } catch {
    // Storage unavailable; the guide may reappear on the next launch.
  }
}

interface UiStore {
  pairTab: PairTab | null;
  openPair: (tab: PairTab) => void;
  closePair: () => void;
  onboardingOpen: boolean;
  openOnboarding: () => void;
  closeOnboarding: () => void;
  dismissOnboarding: () => void;
  evaluateAutoShow: (deviceCount: number) => void;
}

export const useUi = create<UiStore>((set) => ({
  pairTab: null,
  openPair: (pairTab) => set({ pairTab }),
  closePair: () => set({ pairTab: null }),
  onboardingOpen: false,
  openOnboarding: () => set({ onboardingOpen: true }),
  closeOnboarding: () => set({ onboardingOpen: false }),
  dismissOnboarding: () => {
    persistOnboardingDismissed();
    set({ onboardingOpen: false });
  },
  evaluateAutoShow: (deviceCount) => {
    if (deviceCount === 0 && !loadOnboardingDismissed()) set({ onboardingOpen: true });
  },
}));
