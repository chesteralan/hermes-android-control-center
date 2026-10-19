import { beforeEach, describe, expect, it } from "vitest";
import { useDevices } from "./devices";
import { loadTabRoutes, saveTabRoutes, startupRoute, useRoute } from "./route";

describe("tab route persistence", () => {
  beforeEach(() => {
    localStorage.clear();
    useDevices.setState({ activeKey: null });
    useRoute.setState({ route: "dashboard", tabRoutes: {} });
  });

  it("persists per-tab routes to localStorage as views change", () => {
    useDevices.setState({ activeKey: "PHONE_A" });
    useRoute.getState().go("terminal");
    useRoute.getState().selectTab("PHONE_B");
    useRoute.getState().go("logs");
    expect(loadTabRoutes()).toEqual({ PHONE_A: "terminal", PHONE_B: "logs" });
  });

  it("does not record global views", () => {
    useDevices.setState({ activeKey: "PHONE_A" });
    useRoute.getState().go("terminal");
    useRoute.getState().go("settings");
    useRoute.getState().go("overview");
    expect(loadTabRoutes()).toEqual({ PHONE_A: "terminal" });
  });

  it("loadTabRoutes drops invalid keys and global or unknown views", () => {
    saveTabRoutes({
      GOOD: "terminal",
      BAD_GLOBAL: "settings",
      BAD_UNKNOWN: "warpdrive" as never,
    });
    expect(loadTabRoutes()).toEqual({ GOOD: "terminal" });
  });

  it("startupRoute restores the active tab's remembered view", () => {
    expect(startupRoute({ PHONE_A: "logs" }, "PHONE_A")).toBe("logs");
    expect(startupRoute({}, "PHONE_A")).toBe("dashboard");
    expect(startupRoute({ PHONE_A: "logs" }, null)).toBe("dashboard");
    expect(startupRoute({ PHONE_A: "settings" }, "PHONE_A")).toBe("dashboard");
  });
});
