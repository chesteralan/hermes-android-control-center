import { create } from "zustand";
import { useDevices } from "./devices";

export const ROUTES = [
  "dashboard",
  "device",
  "hermes",
  "chat",
  "terminal",
  "logs",
  "overview",
  "settings",
] as const;
export type Route = (typeof ROUTES)[number];

/** Global views are shared across every phone tab; all other views are per tab. */
export function isGlobalRoute(r: Route): boolean {
  return r === "settings" || r === "overview";
}

export const ROUTE_LABELS: Record<Route, string> = {
  dashboard: "Dashboard",
  device: "Device",
  hermes: "Hermes",
  chat: "Chat",
  terminal: "Terminal",
  logs: "Logs",
  overview: "Overview",
  settings: "Settings",
};

interface RouteStore {
  route: Route;
  /** Last device-scoped view per tab key (device_id); restored on tab switch. */
  tabRoutes: Record<string, Route>;
  go: (r: Route) => void;
  /** Select a phone tab: focus it and restore that tab's remembered view. */
  selectTab: (key: string) => void;
}

export const useRoute = create<RouteStore>((set, get) => ({
  route: "dashboard",
  tabRoutes: {},
  go: (route) =>
    set((s) => {
      const key = useDevices.getState().activeKey;
      if (!key || isGlobalRoute(route)) return { route };
      return { route, tabRoutes: { ...s.tabRoutes, [key]: route } };
    }),
  selectTab: (key) => {
    const { route, tabRoutes } = get();
    useDevices.getState().select(key);
    const remembered = tabRoutes[key];
    if (remembered && !isGlobalRoute(route)) set({ route: remembered });
  },
}));
