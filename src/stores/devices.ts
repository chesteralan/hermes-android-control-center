import { create } from "zustand";
import { ipc } from "../lib/ipc";
import type { AdbInfo, AndroidDevice, DeviceInfo, ErrorPayload, ReconnectStatus } from "../types";

export const deviceKey = (d: Pick<AndroidDevice, "deviceId" | "serial">) => d.deviceId ?? d.serial;

interface InfoEntry {
  data: DeviceInfo | null;
  error: ErrorPayload | null;
  loading: boolean;
}

interface AdbStatus {
  info: AdbInfo | null;
  error: ErrorPayload | null;
  checked: boolean;
}

export interface DeviceStore {
  /** Keyed by device_id (falls back to serial) so tabs/history survive wireless port changes. */
  devices: Record<string, AndroidDevice>;
  activeKey: string | null;
  reconnect: Record<string, ReconnectStatus>;
  info: Record<string, InfoEntry>;
  adb: AdbStatus;
  listError: ErrorPayload | null;

  setDevices: (list: AndroidDevice[]) => void;
  setReconnect: (s: ReconnectStatus) => void;
  select: (key: string) => void;
  detectAdb: () => Promise<void>;
  refresh: () => Promise<void>;
  connect: (address: string) => Promise<AndroidDevice>;
  disconnect: (serial: string) => Promise<void>;
  retry: (serial: string) => Promise<void>;
  loadInfo: (key: string) => Promise<void>;
  subscribe: () => () => void;
}

export const useDevices = create<DeviceStore>((set, get) => ({
  devices: {},
  activeKey: null,
  reconnect: {},
  info: {},
  adb: { info: null, error: null, checked: false },
  listError: null,

  setDevices: (list) => {
    const devices: Record<string, AndroidDevice> = {};
    for (const d of list) devices[deviceKey(d)] = d;
    const keys = Object.keys(devices);
    let activeKey = get().activeKey;
    // Never auto-select among many; only when exactly one phone is present.
    if (activeKey && !devices[activeKey]) activeKey = null;
    if (!activeKey && keys.length === 1) activeKey = keys[0] ?? null;
    set({ devices, activeKey, listError: null });
  },

  setReconnect: (s) => set((st) => ({ reconnect: { ...st.reconnect, [s.serial]: s } })),

  select: (key) => set({ activeKey: key }),

  detectAdb: async () => {
    try {
      const info = await ipc.detectAdb();
      set({ adb: { info, error: null, checked: true } });
    } catch (e) {
      set({ adb: { info: null, error: e as ErrorPayload, checked: true } });
    }
  },

  refresh: async () => {
    try {
      get().setDevices(await ipc.listDevices());
    } catch (e) {
      set({ listError: e as ErrorPayload });
      if ((e as ErrorPayload).kind === "adbNotFound") {
        set({ adb: { info: null, error: e as ErrorPayload, checked: true } });
      }
    }
  },

  connect: async (address) => {
    const device = await ipc.connectDevice(address);
    await get().refresh();
    set({ activeKey: deviceKey(device) });
    return device;
  },

  disconnect: async (serial) => {
    await ipc.disconnectDevice(serial);
    await get().refresh();
  },

  retry: async (serial) => {
    await ipc.retryConnection(serial);
  },

  loadInfo: async (key) => {
    const device = get().devices[key];
    if (!device) return;
    set((st) => ({
      info: { ...st.info, [key]: { data: st.info[key]?.data ?? null, error: null, loading: true } },
    }));
    try {
      const data = await ipc.getDeviceInfo(device.serial);
      set((st) => ({ info: { ...st.info, [key]: { data, error: null, loading: false } } }));
    } catch (e) {
      set((st) => ({
        info: {
          ...st.info,
          [key]: { data: st.info[key]?.data ?? null, error: e as ErrorPayload, loading: false },
        },
      }));
    }
  },

  subscribe: () => {
    const unsubs: Array<Promise<() => void>> = [
      ipc.onDevicesChanged((list) => get().setDevices(list)),
      ipc.onReconnect((s) => get().setReconnect(s)),
    ];
    return () => unsubs.forEach((p) => p.then((u) => u()));
  },
}));

export function useActiveDevice(): AndroidDevice | null {
  return useDevices((s) => (s.activeKey ? (s.devices[s.activeKey] ?? null) : null));
}
