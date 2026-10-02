import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useDevices } from "../../stores/devices";
import { useHermes } from "../../stores/hermes";
import { device } from "../../test/fixtures";
import { mockIpc, resetStores } from "../../test/ipcMock";
import type { HermesActionResult, HermesStatus } from "../../types";
import { HermesStatusCard } from "./HermesStatusCard";

const status: HermesStatus = {
  gateway: "running",
  gatewayPid: 18432,
  uptimeSecs: 9060,
  pythonVersion: "Python 3.13.5",
  processes: [
    {
      pid: 18432,
      uptimeSecs: 9060,
      kind: "gateway",
      cmdline: "/usr/local/lib/hermes-agent/venv/bin/python /root/.local/bin/hermes gateway run",
    },
  ],
  platforms: [{ name: "telegram", state: "connected", error: null }],
  supervisor: { running: true, pid: 18400, recentRestarts: 0 },
  warnings: [],
  rawStatusOutput: null,
  source: "termuxSsh",
  checkedAt: 1790960400,
};

const actionResult: HermesActionResult = {
  output: { stdout: "sent TERM to 18432", stderr: "", exitCode: 0, durationMs: 15 },
  status: { ...status, gateway: "stopped", gatewayPid: null, uptimeSecs: null },
  confirmed: true,
};

describe("HermesStatusCard", () => {
  beforeEach(() => {
    resetStores();
    useHermes.setState({ byDevice: {} });
    useDevices.getState().setDevices([device()]);
  });

  it("renders real status, process and platform data and can detect installation", async () => {
    const fn = mockIpc({
      get_hermes_status: () => status,
      detect_hermes: () => ({
        candidates: [
          {
            environment: { type: "prootDistro", distro: "debian" },
            binaryPath: "/root/.local/bin/hermes",
            binDir: "/root/.local/bin",
            version: "Hermes Agent 0.21.5",
            pythonVersion: "Python 3.13.5",
            hermesHomeExists: true,
          },
        ],
        searched: ["termux", "proot:debian"],
      }),
    });
    render(<HermesStatusCard />);
    expect(await screen.findByText("running")).toBeInTheDocument();
    expect(screen.getByText("18432")).toBeInTheDocument();
    expect(screen.getAllByText("2h 31m")).toHaveLength(2);
    expect(screen.getByText("Python 3.13.5")).toBeInTheDocument();
    expect(screen.getByText("telegram")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Detect Hermes" }));
    expect(await screen.findByText("Installed · proot-distro · debian")).toBeInTheDocument();
    expect(screen.getByText("Hermes Agent 0.21.5")).toBeInTheDocument();

    expect(fn).toHaveBeenCalledWith("detect_hermes", { serial: device().serial });
  });

  it("requires confirmation for Stop and names the selected phone", async () => {
    const fn = mockIpc({
      get_hermes_status: () => status,
      hermes_action: () => actionResult,
    });
    render(<HermesStatusCard />);
    await screen.findByText("running");
    await userEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(fn).not.toHaveBeenCalledWith("hermes_action", expect.anything());
    expect(screen.getByRole("dialog", { name: /Stop Hermes on CPH2239/ })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Stop Hermes" }));
    expect(fn).toHaveBeenCalledWith("hermes_action", { serial: device().serial, action: "stop" });
    expect(await screen.findByText("Action confirmed")).toBeInTheDocument();
  });

  it("renders limited ADB-only status and warnings honestly", async () => {
    mockIpc({
      get_hermes_status: () => ({
        ...status,
        source: "adb",
        pythonVersion: null,
        platforms: [],
        supervisor: { running: false, pid: null, recentRestarts: 0 },
        warnings: ["Termux bridge not connected — showing process info from ADB only."],
      }),
    });
    render(<HermesStatusCard />);
    expect(await screen.findByText("ADB (limited)")).toBeInTheDocument();
    expect(screen.getByText(/Termux bridge not connected/)).toBeInTheDocument();
  });

  it("keeps unknown fields as Unknown", async () => {
    mockIpc({
      get_hermes_status: () => ({
        ...status,
        gateway: "unknown",
        gatewayPid: null,
        uptimeSecs: null,
        pythonVersion: null,
        processes: [],
        platforms: [],
        supervisor: { running: false, pid: null, recentRestarts: 0 },
      }),
    });
    render(<HermesStatusCard />);
    expect(await screen.findByText("unknown")).toBeInTheDocument();
    expect(screen.getAllByText("Unknown").length).toBeGreaterThanOrEqual(3);
  });

  it("shows configured status output in expandable details", async () => {
    mockIpc({
      get_hermes_status: () => ({ ...status, rawStatusOutput: "Hermes: running\\nGateway: healthy" }),
    });
    render(<HermesStatusCard />);
    await screen.findByText("running");
    await userEvent.click(screen.getByText("Status command output"));
    expect(screen.getByText(/Gateway: healthy/)).toBeInTheDocument();
  });
  it("runs Doctor directly and requires confirmation before Update", async () => {
    const fn = mockIpc({
      get_hermes_status: () => status,
      run_hermes_tool: () => ({
        stdout: "doctor passed",
        stderr: "",
        exitCode: 0,
        durationMs: 150,
      }),
    });
    render(<HermesStatusCard />);
    await screen.findByText("running");
    await userEvent.click(screen.getByRole("button", { name: "Doctor" }));
    expect(fn).toHaveBeenCalledWith("run_hermes_tool", { serial: device().serial, tool: "doctor" });
    expect(await screen.findByText("doctor passed")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Update" }));
    expect(fn).not.toHaveBeenCalledWith("run_hermes_tool", {
      serial: device().serial,
      tool: "update",
    });
    expect(screen.getByRole("dialog", { name: /Update Hermes on CPH2239/ })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Update Hermes" }));
    expect(fn).toHaveBeenCalledWith("run_hermes_tool", { serial: device().serial, tool: "update" });
  });
});
