import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useDevices } from "../../stores/devices";
import { useUi } from "../../stores/ui";
import { device, info } from "../../test/fixtures";
import { mockIpc, resetStores } from "../../test/ipcMock";
import type { AndroidDevice, QrPairEvent } from "../../types";
import { ConnectPanel } from "./ConnectPanel";
import { DeviceInfoCard } from "./DeviceInfoCard";
import { DeviceList } from "./DeviceList";
import { deviceStatus } from "./deviceStatus";
import { NoDeviceState } from "./NoDeviceState";
import { QrTab } from "./PairDialog";
import { DashboardView } from "../dashboard/DashboardView";
import { useRoute } from "../../stores/route";

beforeEach(() => resetStores());

describe("deviceStatus", () => {
  it.each<[AndroidDevice["state"], string]>([
    ["device", "Connected"],
    ["unauthorized", "Unauthorized"],
    ["offline", "Offline"],
    ["connecting", "Connecting"],
    ["disconnected", "Disconnected"],
  ])("%s → %s", (state, label) => {
    expect(deviceStatus(device({ state })).label).toBe(label);
  });

  it("shows reconnect progress", () => {
    const s = deviceStatus(device({ state: "reconnecting" }), {
      serial: "s",
      deviceId: null,
      phase: "waiting",
      attempt: 2,
      maxAttempts: 8,
      nextDelayMs: 5000,
    });
    expect(s.label).toBe("Reconnecting (attempt 2/8, next in 5s)");
  });
});

describe("devices store", () => {
  it("auto-selects only when exactly one device is present", () => {
    useDevices.getState().setDevices([device()]);
    expect(useDevices.getState().activeKey).toBe("TESTSERIAL0001");
    useDevices.setState({ activeKey: null });
    useDevices.getState().setDevices([device(), device({ serial: "B", deviceId: "B" })]);
    expect(useDevices.getState().activeKey).toBeNull();
  });
});

describe("ConnectPanel", () => {
  it("validates IP and port before calling adb", async () => {
    const fn = mockIpc({});
    render(<ConnectPanel />);
    await userEvent.type(screen.getByLabelText("IP address"), "999.1.1.1");
    await userEvent.type(screen.getByLabelText("Port"), "abc");
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(screen.getByText("Invalid IPv4 address.")).toBeInTheDocument();
    expect(screen.getByText("Port must be a number.")).toBeInTheDocument();
    expect(fn).not.toHaveBeenCalled();
  });

  it("connects and shows a readable error with details", async () => {
    const fn = mockIpc({
      connect_device: () => {
        throw {
          kind: "connectionRefused",
          message: "Unable to connect to 192.0.2.25:5555: connection refused.",
          details: "ADB returned:\nfailed to connect",
        };
      },
    });
    render(<ConnectPanel />);
    await userEvent.type(screen.getByLabelText("IP address"), "192.0.2.25");
    await userEvent.type(screen.getByLabelText("Port"), "5555");
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(fn).toHaveBeenCalledWith("connect_device", { address: "192.0.2.25:5555" });
    expect(await screen.findByRole("alert")).toHaveTextContent("connection refused");
  });
});

describe("DeviceList", () => {
  it("renders every connection state and offers Retry after give-up", async () => {
    const retry = mockIpc({ retry_connection: () => null });
    useDevices
      .getState()
      .setDevices([
        device({ serial: "A", deviceId: "A", state: "device" }),
        device({ serial: "B", deviceId: "B", state: "unauthorized" }),
        device({ serial: "C", deviceId: "C", state: "disconnected", connection: "wirelessIp" }),
      ]);
    render(<DeviceList />);
    expect(screen.getByText("Connected")).toBeInTheDocument();
    expect(screen.getByText("Unauthorized")).toBeInTheDocument();
    expect(screen.getByText(/Allow debugging/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(retry).toHaveBeenCalledWith("retry_connection", { serial: "C" });
  });

  it("asks for confirmation before disconnecting", async () => {
    const fn = mockIpc({ disconnect_device: () => null, list_devices: () => [] });
    useDevices.getState().setDevices([device()]);
    render(<DeviceList />);
    await userEvent.click(screen.getByRole("button", { name: "Disconnect" }));
    expect(fn).not.toHaveBeenCalledWith("disconnect_device", expect.anything());
    const dialog = screen.getByRole("dialog");
    await userEvent.click(dialog.querySelector("button:last-child") as HTMLButtonElement);
    expect(fn).toHaveBeenCalledWith("disconnect_device", { serial: device().serial });
  });
});

describe("DeviceInfoCard", () => {
  it("renders full device information", async () => {
    mockIpc({ get_device_info: () => info() });
    useDevices.getState().setDevices([device()]);
    render(<DeviceInfoCard device={device()} />);
    expect(await screen.findByText("82% · charging")).toBeInTheDocument();
    expect(screen.getByText("48 GB free of 100 GB")).toBeInTheDocument();
    expect(screen.getByText("3.2 GB / 8.0 GB")).toBeInTheDocument();
    expect(screen.getByText("11")).toBeInTheDocument();
    expect(screen.getByText(/0.119.0-beta.3/)).toBeInTheDocument();
  });

  it("shows Unknown instead of fabricating missing data", async () => {
    mockIpc({
      get_device_info: () =>
        info({
          battery: null,
          storage: null,
          memory: null,
          cpu: null,
          termux: null,
          androidVersion: null,
        }),
    });
    useDevices.getState().setDevices([device()]);
    render(<DeviceInfoCard device={device()} />);
    await screen.findByText("TESTSERIAL0001");
    expect(screen.getAllByText("Unknown").length).toBeGreaterThanOrEqual(5);
  });

  it("shows errors with retry", async () => {
    mockIpc({
      get_device_info: () => {
        throw {
          kind: "deviceOffline",
          message: "Device X is offline. Wake the phone and check Wi-Fi.",
          details: null,
        };
      },
    });
    useDevices.getState().setDevices([device()]);
    render(<DeviceInfoCard device={device()} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("offline");
  });
});

describe("NoDeviceState", () => {
  it("offers pairing and connection actions", async () => {
    render(<NoDeviceState />);
    for (const name of [
      "Pair a phone to set it up",
      "Pair with code",
      "Connect by IP",
      "Discover",
    ]) {
      expect(screen.getByRole("button", { name })).toBeInTheDocument();
    }
    await userEvent.click(screen.getByRole("button", { name: "Pair a phone to set it up" }));
    expect(useUi.getState().pairTab).toBe("qr");
    await userEvent.click(screen.getByRole("button", { name: "Pair with code" }));
    expect(useUi.getState().pairTab).toBe("code");
  });

  it("offers a Dashboard setup entry when connected-device info confirms Termux is absent", async () => {
    mockIpc({
      get_device_info: () => info({ termux: { ...info().termux!, installed: false } }),
      get_hermes_status: () => null,
    });
    useDevices.getState().setDevices([device()]);
    render(<DashboardView />);

    expect(await screen.findByText("Termux is not installed on this phone.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Open setup wizard" }));
    expect(useRoute.getState().route).toBe("device");
  });
});

describe("QR pairing", () => {
  it("shows the QR code and follows pairing events", async () => {
    let channel: { onmessage: (e: QrPairEvent) => void } | undefined;
    mockIpc({
      start_qr_pairing: (args) => {
        channel = args?.onEvent as typeof channel;
        return { sessionId: "hacc-abc", qrSvg: "<svg></svg>", expiresInMs: 120000 };
      },
      cancel_qr_pairing: () => null,
      list_devices: () => [],
    });
    let done = false;
    render(<QrTab onUseCode={() => {}} onDone={() => (done = true)} />);
    expect(await screen.findByAltText("Pairing QR code")).toBeInTheDocument();
    channel?.onmessage({ type: "waiting", remainingMs: 90000 });
    expect(await screen.findByText(/Waiting for phone… \(1m 30s left\)/)).toBeInTheDocument();
    channel?.onmessage({ type: "found", address: "192.0.2.25:40111" });
    expect(await screen.findByText(/pairing…/)).toBeInTheDocument();
    channel?.onmessage({ type: "connected", address: "192.0.2.25:44467" });
    await screen.findByText(/Connected to/);
    expect(done).toBe(true);
  });

  it("offers Regenerate after expiry", async () => {
    let channel: { onmessage: (e: QrPairEvent) => void } | undefined;
    mockIpc({
      start_qr_pairing: (args) => {
        channel = args?.onEvent as typeof channel;
        return { sessionId: "hacc-abc", qrSvg: "<svg></svg>", expiresInMs: 120000 };
      },
      cancel_qr_pairing: () => null,
    });
    render(<QrTab onUseCode={() => {}} onDone={() => {}} />);
    await screen.findByAltText("Pairing QR code");
    channel?.onmessage({ type: "expired" });
    expect(await screen.findByRole("button", { name: "Regenerate" })).toBeInTheDocument();
  });
});
