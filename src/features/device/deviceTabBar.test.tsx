import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useDeviceProfiles } from "../../stores/deviceProfiles";
import { ACTIVE_KEY_STORAGE, loadActiveKey, useDevices } from "../../stores/devices";
import { useRoute } from "../../stores/route";
import { device } from "../../test/fixtures";
import { mockIpc, resetStores } from "../../test/ipcMock";
import type { DeviceProfile } from "../../types";
import { DeviceTabBar, mergeTabOrder } from "./DeviceTabBar";

// ─── helpers ──────────────────────────────────────────────────────────────────

function makeProfile(over: Partial<DeviceProfile> = {}): DeviceProfile {
  return {
    deviceId: "TESTSERIAL0001",
    alias: null,
    color: null,
    lastAddress: null,
    autoConnect: true,
    hermes: null,
    termux: null,
    logCommand: null,
    ...over,
  };
}

const deviceA = device({
  serial: "adb-A-1._adb-tls-connect._tcp",
  deviceId: "PHONE_A",
  model: "Pixel7",
});
const deviceB = device({
  serial: "adb-B-1._adb-tls-connect._tcp",
  deviceId: "PHONE_B",
  model: "Galaxy",
});
const deviceC = device({
  serial: "adb-C-1._adb-tls-connect._tcp",
  deviceId: "PHONE_C",
  model: "Moto",
  state: "offline",
});

// ─── mergeTabOrder unit tests ─────────────────────────────────────────────────

describe("mergeTabOrder", () => {
  it("preserves saved order for known keys", () => {
    expect(mergeTabOrder(["B", "A"], ["A", "B"])).toEqual(["B", "A"]);
  });

  it("appends new keys at the end in current order", () => {
    expect(mergeTabOrder(["A"], ["A", "B", "C"])).toEqual(["A", "B", "C"]);
  });

  it("drops keys that are no longer present", () => {
    expect(mergeTabOrder(["A", "X", "B"], ["A", "B"])).toEqual(["A", "B"]);
  });

  it("returns current order when no saved order", () => {
    expect(mergeTabOrder([], ["A", "B"])).toEqual(["A", "B"]);
  });
});

// ─── component tests ──────────────────────────────────────────────────────────

describe("DeviceTabBar", () => {
  beforeEach(() => {
    resetStores();
    localStorage.clear();
  });

  it("renders nothing when fewer than 2 devices are present", () => {
    useDevices.getState().setDevices([deviceA]);
    const { container } = render(<DeviceTabBar />);
    expect(container.firstChild).toBeNull();
  });

  it("renders a tab per device when ≥ 2 devices are present", () => {
    useDevices.getState().setDevices([deviceA, deviceB]);
    render(<DeviceTabBar />);
    const tabs = screen.getAllByRole("tab");
    expect(tabs).toHaveLength(2);
    // Tabs contain device models as fallback label.
    expect(screen.getByText("Pixel7")).toBeInTheDocument();
    expect(screen.getByText("Galaxy")).toBeInTheDocument();
  });

  it("uses alias from device profile when available", () => {
    useDevices.getState().setDevices([deviceA, deviceB]);
    useDeviceProfiles.setState({
      profiles: {
        PHONE_A: makeProfile({ deviceId: "PHONE_A", alias: "My Pixel" }),
      },
      profileList: [],
      effective: {},
      loading: false,
      error: null,
    });
    render(<DeviceTabBar />);
    expect(screen.getByText("My Pixel")).toBeInTheDocument();
    expect(screen.getByText("Galaxy")).toBeInTheDocument();
  });

  it("marks the active tab with aria-selected", () => {
    useDevices.getState().setDevices([deviceA, deviceB]);
    useDevices.setState({ activeKey: "PHONE_A" });
    render(<DeviceTabBar />);
    const tabs = screen.getAllByRole("tab");
    const activeTab = tabs.find((t) => t.getAttribute("aria-selected") === "true");
    expect(activeTab).toBeInTheDocument();
    expect(activeTab).toHaveTextContent("Pixel7");
  });

  it("clicking a tab selects that device", async () => {
    useDevices.getState().setDevices([deviceA, deviceB]);
    render(<DeviceTabBar />);
    await userEvent.click(screen.getByText("Galaxy"));
    expect(useDevices.getState().activeKey).toBe("PHONE_B");
  });

  it("shows connection state badge on each tab", () => {
    useDevices.getState().setDevices([deviceA, deviceC]);
    render(<DeviceTabBar />);
    expect(screen.getByText("Connected")).toBeInTheDocument();
    expect(screen.getByText("Offline")).toBeInTheDocument();
  });

  it("renders the tablist with an accessible label", () => {
    useDevices.getState().setDevices([deviceA, deviceB]);
    render(<DeviceTabBar />);
    expect(screen.getByRole("tablist", { name: "Connected devices" })).toBeInTheDocument();
  });

  it("overflow menu opens on click and shows available actions", async () => {
    useDevices.getState().setDevices([deviceA, deviceB]);
    render(<DeviceTabBar />);
    const moreBtn = screen.getAllByRole("button", { name: /More options/ })[0]!;
    await userEvent.click(moreBtn);
    expect(screen.getByRole("menu")).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: /Disconnect/ })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: /Open profile/ })).toBeInTheDocument();
    // A connected device has nothing to retry.
    expect(screen.queryByRole("menuitem", { name: /Retry/ })).not.toBeInTheDocument();
  });

  it("overflow menu calls onOpenProfile when 'Open profile' is clicked", async () => {
    const onOpen = vi.fn();
    useDevices.getState().setDevices([deviceA, deviceB]);
    render(<DeviceTabBar onOpenProfile={onOpen} />);
    const moreBtn = screen.getAllByRole("button", { name: /More options/ })[0]!;
    await userEvent.click(moreBtn);
    await userEvent.click(screen.getByRole("menuitem", { name: /Open profile/ }));
    expect(onOpen).toHaveBeenCalledWith("PHONE_A");
  });

  it("overflow menu shows Retry for disconnected devices", async () => {
    const disconnected = device({
      serial: "adb-D-1._adb-tls-connect._tcp",
      deviceId: "PHONE_D",
      model: "NothingPhone",
      state: "disconnected",
    });
    useDevices.getState().setDevices([deviceA, disconnected]);
    render(<DeviceTabBar />);
    const moreBtns = screen.getAllByRole("button", { name: /More options/ });
    await userEvent.click(moreBtns[1]!);
    expect(screen.getByRole("menuitem", { name: /Retry/ })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /Disconnect/ })).not.toBeInTheDocument();
  });

  it("disconnect confirmation dialog names the target device", async () => {
    mockIpc({
      disconnect_device: () => undefined,
      list_devices: () => [],
    });
    useDevices.getState().setDevices([deviceA, deviceB]);
    render(<DeviceTabBar />);
    const moreBtn = screen.getAllByRole("button", { name: /More options/ })[0]!;
    await userEvent.click(moreBtn);
    await userEvent.click(screen.getByRole("menuitem", { name: /Disconnect/ }));
    const dialog = await screen.findByRole("dialog");
    // Dialog title names the device model.
    expect(dialog).toHaveTextContent("Pixel7");
  });

  it("persists tab order to localStorage", () => {
    useDevices.getState().setDevices([deviceA, deviceB, deviceC]);
    render(<DeviceTabBar />);
    // After render the order should be saved.
    const saved = JSON.parse(localStorage.getItem("hacc.deviceTabOrder") ?? "[]");
    expect(saved).toEqual(expect.arrayContaining(["PHONE_A", "PHONE_B", "PHONE_C"]));
  });

  it("port-change rebind: tab for old key stays when deviceId is same", () => {
    // Device A changes serial (port change) but keeps the same deviceId.
    const aOriginal = device({ serial: "192.0.2.10:5555", deviceId: "PHONE_A", model: "Pixel7" });
    const aRebind = device({ serial: "192.0.2.10:5559", deviceId: "PHONE_A", model: "Pixel7" });
    useDevices.getState().setDevices([aOriginal, deviceB]);
    const { rerender } = render(<DeviceTabBar />);
    expect(screen.getAllByRole("tab")).toHaveLength(2);
    // Simulate port change: new serial but same deviceId.
    useDevices.getState().setDevices([aRebind, deviceB]);
    rerender(<DeviceTabBar />);
    // Still 2 tabs, and Pixel7 is still present.
    expect(screen.getAllByRole("tab")).toHaveLength(2);
    expect(screen.getByText("Pixel7")).toBeInTheDocument();
  });

  it("shows reconnecting and unknown connection states", () => {
    const reconnecting = device({
      serial: "adb-R-1._adb-tls-connect._tcp",
      deviceId: "PHONE_R",
      model: "Pixel8",
      state: "reconnecting",
    });
    const unknown = device({
      serial: "adb-U-1._adb-tls-connect._tcp",
      deviceId: "PHONE_U",
      model: "Mystery",
      state: "unknown",
      rawState: "",
    });
    useDevices.getState().setDevices([reconnecting, unknown]);
    render(<DeviceTabBar />);
    expect(screen.getByText("Reconnecting")).toBeInTheDocument();
    expect(screen.getByText("Unknown")).toBeInTheDocument();
  });

  it("restores a persisted tab order across restarts", () => {
    localStorage.setItem("hacc.deviceTabOrder", JSON.stringify(["PHONE_B", "PHONE_A"]));
    useDevices.getState().setDevices([deviceA, deviceB]);
    render(<DeviceTabBar />);
    const labels = screen.getAllByRole("tab").map((t) => t.textContent ?? "");
    expect(labels[0]).toContain("Galaxy");
    expect(labels[1]).toContain("Pixel7");
  });

  it("persists the last-selected tab key", async () => {
    useDevices.getState().setDevices([deviceA, deviceB]);
    render(<DeviceTabBar />);
    await userEvent.click(screen.getByText("Galaxy"));
    expect(localStorage.getItem(ACTIVE_KEY_STORAGE)).toBe("PHONE_B");
    expect(loadActiveKey()).toBe("PHONE_B");
  });

  it("keeps a separate view per tab and restores it on switch", async () => {
    useDevices.getState().setDevices([deviceA, deviceB]);
    render(<DeviceTabBar />);
    await userEvent.click(screen.getByText("Pixel7"));
    useRoute.getState().go("terminal");
    await userEvent.click(screen.getByText("Galaxy"));
    // A tab with no remembered view inherits the current one.
    expect(useRoute.getState().route).toBe("terminal");
    useRoute.getState().go("logs");
    await userEvent.click(screen.getByText("Pixel7"));
    expect(useRoute.getState().route).toBe("terminal");
    await userEvent.click(screen.getByText("Galaxy"));
    expect(useRoute.getState().route).toBe("logs");
  });

  it("leaves a global view unchanged when switching tabs", async () => {
    useDevices.getState().setDevices([deviceA, deviceB]);
    render(<DeviceTabBar />);
    await userEvent.click(screen.getByText("Pixel7"));
    useRoute.getState().go("settings");
    await userEvent.click(screen.getByText("Galaxy"));
    expect(useRoute.getState().route).toBe("settings");
  });
});
