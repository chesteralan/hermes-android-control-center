import { create } from "zustand";
import { ipc } from "../lib/ipc";
import type {
  DeviceProfile,
  DeviceProfileUpdate,
  EffectiveDeviceConfig,
  ErrorPayload,
} from "../types";

function sortedList(profiles: Record<string, DeviceProfile>): DeviceProfile[] {
  return Object.values(profiles).sort((a, b) => a.deviceId.localeCompare(b.deviceId));
}

export interface DeviceProfilesStore {
  /** All known device profiles, keyed by deviceId. */
  profiles: Record<string, DeviceProfile>;
  /** Sorted list derived from profiles — kept stable to avoid selector loops. */
  profileList: DeviceProfile[];
  /** Effective (merged) config per deviceId, keyed by deviceId. Populated on demand. */
  effective: Record<string, EffectiveDeviceConfig>;
  loading: boolean;
  error: ErrorPayload | null;

  /** Load all profiles from the backend. */
  load: () => Promise<void>;
  /** Reload the effective config for a single device. */
  loadEffective: (deviceId: string) => Promise<void>;
  /** Partially update a profile (creates an entry first if it does not exist). */
  set: (deviceId: string, update: Partial<DeviceProfileUpdate>) => Promise<DeviceProfile>;
  /** Delete a profile. */
  delete: (deviceId: string) => Promise<void>;
  /** Clear any error. */
  clearError: () => void;
}

export const useDeviceProfiles = create<DeviceProfilesStore>((set) => ({
  profiles: {},
  profileList: [],
  effective: {},
  loading: false,
  error: null,

  load: async () => {
    set({ loading: true, error: null });
    try {
      const list = await ipc.listDeviceProfiles();
      const profiles: Record<string, DeviceProfile> = {};
      for (const p of list) profiles[p.deviceId] = p;
      set({ profiles, profileList: sortedList(profiles), loading: false });
    } catch (e) {
      set({ error: e as ErrorPayload, loading: false });
    }
  },

  loadEffective: async (deviceId: string) => {
    try {
      const eff = await ipc.getEffectiveDeviceConfig(deviceId);
      set((s) => ({ effective: { ...s.effective, [deviceId]: eff } }));
    } catch (e) {
      set({ error: e as ErrorPayload });
    }
  },

  set: async (deviceId, update) => {
    set({ error: null });
    const profile = await ipc.setDeviceProfile(deviceId, update);
    set((s) => {
      const profiles = { ...s.profiles, [deviceId]: profile };
      const effective = Object.fromEntries(
        Object.entries(s.effective).filter(([id]) => id !== deviceId),
      );
      return { profiles, profileList: sortedList(profiles), effective };
    });
    return profile;
  },

  delete: async (deviceId) => {
    set({ error: null });
    await ipc.deleteDeviceProfile(deviceId);
    set((s) => {
      const profiles = { ...s.profiles };
      delete profiles[deviceId];
      const effective = { ...s.effective };
      delete effective[deviceId];
      return { profiles, profileList: sortedList(profiles), effective };
    });
  },

  clearError: () => set({ error: null }),
}));

/** Sorted list of profiles for rendering. */
export function useProfileList(): DeviceProfile[] {
  return useDeviceProfiles((s) => s.profileList);
}
