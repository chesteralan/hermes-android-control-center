import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useDevices } from "../../stores/devices";
import { useHermes } from "../../stores/hermes";
import { useRoute } from "../../stores/route";
import { device, info } from "../../test/fixtures";
import { mockIpc, resetStores } from "../../test/ipcMock";
import type { HermesStatus } from "../../types";
import { OverviewView } from "./OverviewView";

const runningStatus: HermesStatus = {
  gateway: "running",
  gatewayPid: 42,
  uptimeSecs: 120,
  pythonVersion: "3.11.9",
  processes: [],
  platforms: [],
  supervisor: { running: true, pid: 42, recentRestarts: 0 },
  warnings: [],
  rawStatusOutput: null,
  source: "adb",
  checkedAt: 1,
};

describe("OverviewView", () => {
  beforeEach(() => {
    resetStores();
    useHermes.setState({ byDevice: {} });
  });

  it("renders a status card per phone and never lets one phone's pending fetch gate another", async () => {
    const phoneA = device({ serial: "serial-A", deviceId: "PHONE_A", model: "Pixel7" });
    const phoneB = device({ serial: "serial-B", deviceId: "PHONE_B", model: "Galaxy" });
    const phoneC = device({
      serial: "serial-C",
      deviceId: "PHONE_C",
      model: "Moto",
      state: "offline",
    });
    mockIpc({
      get_device_info: (args) => {
        const serial = String(args?.serial);
        if (serial === "serial-A") return info({ serial, deviceId: "PHONE_A" });
        if (serial === "serial-B") {
          throw { kind: "io", message: "info unavailable", details: null };
        }
        // Phone C's info request never settles.
        return new Promise(() => {});
      },
      get_hermes_status: (args) => {
        if (String(args?.serial) === "serial-A") return runningStatus;
        throw { kind: "io", message: "device offline", details: null };
      },
    });
    useDevices.getState().setDevices([phoneA, phoneB, phoneC]);

    render(<OverviewView />);

    // Phone A's battery/storage/gateway appear even though C is still pending.
    expect(await screen.findByText(/82%/)).toBeInTheDocument();
    expect(screen.getByText(/48\.0 GB free of 100\.0 GB/)).toBeInTheDocument();
    expect(screen.getByText(/Hermes gateway: running/)).toBeInTheDocument();
    // Phone B reports its info error without blocking the others.
    expect(screen.getByText("no info")).toBeInTheDocument();
    // Phone C is still loading its own snapshot.
    expect(screen.getAllByText("loading…").length).toBeGreaterThan(0);
    expect(screen.getByText("Offline")).toBeInTheDocument();
  });

  it("links a card to the phone's tab", async () => {
    mockIpc({
      get_device_info: () => info(),
      get_hermes_status: () => null,
    });
    const phoneA = device({ serial: "serial-A", deviceId: "PHONE_A" });
    const phoneB = device({ serial: "serial-B", deviceId: "PHONE_B", model: "Galaxy" });
    useDevices.getState().setDevices([phoneA, phoneB]);
    useRoute.setState({ route: "overview" });

    render(<OverviewView />);
    const openButtons = await screen.findAllByRole("button", { name: "Open" });
    await userEvent.click(openButtons[1]!);

    expect(useDevices.getState().activeKey).toBe("PHONE_B");
    expect(useRoute.getState().route).toBe("device");
  });

  it("shows the latest reconnect attempt on the card", async () => {
    mockIpc({
      get_device_info: () => info(),
      get_hermes_status: () => null,
    });
    const phone = device({
      serial: "serial-A",
      deviceId: "PHONE_A",
      model: "Pixel7",
      state: "reconnecting",
    });
    useDevices.getState().setDevices([phone]);
    useDevices.getState().setReconnect({
      serial: "serial-A-new",
      deviceId: "PHONE_A",
      phase: "waiting",
      attempt: 3,
      maxAttempts: 8,
      nextDelayMs: 1_000,
    });

    render(<OverviewView />);
    expect(await screen.findByText("Reconnecting (attempt 3/8)")).toBeInTheDocument();
  });

  it("shows an empty state when no phones are known", () => {
    render(<OverviewView />);
    expect(screen.getByText("No phone connected")).toBeInTheDocument();
  });
});
