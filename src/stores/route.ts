import { create } from "zustand";

export const ROUTES = [
  "dashboard",
  "device",
  "hermes",
  "chat",
  "terminal",
  "logs",
  "settings",
] as const;
export type Route = (typeof ROUTES)[number];

export const ROUTE_LABELS: Record<Route, string> = {
  dashboard: "Dashboard",
  device: "Device",
  hermes: "Hermes",
  chat: "Chat",
  terminal: "Terminal",
  logs: "Logs",
  settings: "Settings",
};

interface RouteStore {
  route: Route;
  go: (r: Route) => void;
}

export const useRoute = create<RouteStore>((set) => ({
  route: "dashboard",
  go: (route) => set({ route }),
}));
