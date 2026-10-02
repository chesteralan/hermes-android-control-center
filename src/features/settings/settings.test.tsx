import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useDevices } from "../../stores/devices";
import { useSettings } from "../../stores/settings";
import { device } from "../../test/fixtures";
import { mockIpc, resetStores } from "../../test/ipcMock";
import type { AppConfig } from "../../types";
import { previewHermesCommand, SettingsView } from "./SettingsView";

const cfg: AppConfig = {
  version: 1,
  adbPath: null,
  knownAddresses: [],
  reconnect: { enabled: true, scheduleMs: [1000, 2000] },
  hermes: {
    environment: { type: "prootDistro", distro: "debian" },
    startMode: "supervised",
    gatewayCommand: "hermes gateway run",
    startCommand: "",
    stopCommand: "",
    restartCommand: "",
    statusCommand: "",
    logCommand: "",
    versionCommand: "hermes --version",
    doctorCommand: "hermes doctor",
    updateCommand: "hermes update",
    processMatch: "hermes-agent/venv/bin/python",
    gatewayMatch: "gateway run",
    hermesHome: "/root/.hermes",
    pathPrepend: ["/root/.local/bin"],
  },
  logs: { autoStart: false, logcatFilter: "*:I" },
  termux: { sshUser: "termux", sshPort: 8022 },
  apiPort: 8765,
  logLevel: "info",
};

describe("SettingsView", () => {
  beforeEach(() => resetStores());

  it("saves edited settings", async () => {
    const fn = mockIpc({
      get_settings: () => cfg,
      update_settings: (args) => args?.config,
    });
    render(<SettingsView />);
    const start = await screen.findByLabelText("Start override");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    await userEvent.type(start, "hermes gateway run");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(fn).toHaveBeenCalledWith("update_settings", {
      config: { ...cfg, hermes: { ...cfg.hermes, startCommand: "hermes gateway run" } },
    });
  });

  it("shows validation errors from the backend", async () => {
    mockIpc({
      get_settings: () => cfg,
      update_settings: () => {
        throw { kind: "config", message: "ADB path does not exist: /nope", details: null };
      },
    });
    render(<SettingsView />);
    await userEvent.type(await screen.findByLabelText("ADB path"), "/nope");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("ADB path does not exist: /nope")).toBeInTheDocument();
  });

  it("previews the configured environment wrapper", () => {
    expect(previewHermesCommand(cfg.hermes)).toBe(
      "proot-distro login 'debian' -- bash -c 'export PATH='\\''/root/.local/bin'\\'':$PATH; hermes gateway run'",
    );
  });

  it("adopts a detected Debian Hermes install", async () => {
    resetStores();
    useDevices.getState().setDevices([device()]);
    mockIpc({
      get_settings: () => cfg,
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
    render(<SettingsView />);
    await userEvent.click(await screen.findByRole("button", { name: "Detect Hermes" }));
    await userEvent.click(await screen.findByRole("button", { name: "Use this" }));
    expect(useSettings.getState().draft?.hermes.environment).toEqual({
      type: "prootDistro",
      distro: "debian",
    });
    expect(useSettings.getState().draft?.hermes.pathPrepend).toEqual(["/root/.local/bin"]);
  });
});
