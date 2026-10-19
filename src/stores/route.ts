import { create } from "zustand";
import { loadActiveKey, useDevices } from "./devices";

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

/** localStorage key for per-tab view memory so it survives a restart. */
const TAB_ROUTES_STORAGE = "hacc.tabRoutes";

function isRoute(value: unknown): value is Route {
  return typeof value === "string" && (ROUTES as readonly string[]).includes(value);
}

export function loadTabRoutes(): Record<string, Route> {
  try {
    const raw = localStorage.getItem(TAB_ROUTES_STORAGE);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null) return {};
    const out: Record<string, Route> = {};
    for (const [key, value] of Object.entries(parsed)) {
      // Global views never belong to a tab.
      if (isRoute(value) && !isGlobalRoute(value)) out[key] = value;
    }
    return out;
  } catch {
    return {};
  }
}

export function saveTabRoutes(map: Record<string, Route>): void {
  try {
    localStorage.setItem(TAB_ROUTES_STORAGE, JSON.stringify(map));
  } catch {
    // Storage might be unavailable; in-memory behaviour is unchanged.
  }
}

/** The view a restart should open: the active tab's remembered device view. */
export function startupRoute(tabRoutes: Record<string, Route>, activeKey: string | null): Route {
  const remembered = activeKey ? tabRoutes[activeKey] : undefined;
  return remembered && !isGlobalRoute(remembered) ? remembered : "dashboard";
}

const initialTabRoutes = loadTabRoutes();

interface RouteStore {
  route: Route;
  /** Last device-scoped view per tab key (device_id); restored on tab switch. */
  tabRoutes: Record<string, Route>;
  go: (r: Route) => void;
  /** Select a phone tab: focus it and restore that tab's remembered view. */
  selectTab: (key: string) => void;
}

export const useRoute = create<RouteStore>((set, get) => ({
  route: startupRoute(initialTabRoutes, loadActiveKey()),
  tabRoutes: initialTabRoutes,
  go: (route) => {
    const key = useDevices.getState().activeKey;
    if (!key || isGlobalRoute(route)) {
      set({ route });
      return;
    }
    const tabRoutes = { ...get().tabRoutes, [key]: route };
    set({ route, tabRoutes });
    saveTabRoutes(tabRoutes);
  },
  selectTab: (key) => {
    const { route, tabRoutes } = get();
    useDevices.getState().select(key);
    const remembered = tabRoutes[key];
    if (remembered && !isGlobalRoute(route)) set({ route: remembered });
  },
}));
