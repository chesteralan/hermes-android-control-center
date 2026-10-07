import { fireEvent, render, screen, waitFor } from "@testing-library/react";
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
    transport: "termuxSsh",
    fallbackToSsh: false,
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
    logFiles: ["/root/.hermes/logs/gateway.log", "/root/.hermes/logs/tool_calls.log"],
    pathPrepend: ["/root/.local/bin"],
  },
  logs: { autoStart: false, logcatFilter: "*:I" },
  termux: { sshUser: "termux", sshPort: 8022 },
  apiPort: 8765,
  logLevel: "info",
};

describe("SettingsView", () => {
  beforeEach(() => resetStores());

  it("requires explicit vault consent and clears the passphrase after unlock", async () => {
    const invoke = mockIpc({
      get_settings: () => cfg,
      get_secret_storage_state: () => "native",
      unlock_secret_storage: () => undefined,
      lock_secret_storage: () => undefined,
    });
    render(<SettingsView />);
    await screen.findByText("System credential store");
    const passphrase = screen.getByLabelText("Vault passphrase");
    fireEvent.change(passphrase, { target: { value: "test vault passphrase" } });
    const enable = screen.getByRole("button", { name: "Enable encrypted storage" });
    expect(enable).toBeDisabled();
    await userEvent.click(
      screen.getByRole("checkbox", { name: "Use passphrase-encrypted file storage" }),
    );
    await userEvent.click(enable);
    expect(invoke).toHaveBeenCalledWith("unlock_secret_storage", {
      passphrase: "test vault passphrase",
      optedIn: true,
    });
    await screen.findByText("Encrypted file - unlocked");
    expect(screen.queryByLabelText("Vault passphrase")).not.toBeInTheDocument();
    expect(JSON.stringify(useSettings.getState().saved)).not.toContain("test vault passphrase");
    await userEvent.click(screen.getByRole("button", { name: "Lock vault" }));
    expect(await screen.findByLabelText("Vault passphrase")).toHaveValue("");
    expect(screen.getByRole("button", { name: "Unlock vault" })).toBeDisabled();
  });

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

  it("edits Hermes log file paths", async () => {
    const fn = mockIpc({
      get_settings: () => cfg,
      update_settings: (args) => args?.config,
    });
    render(<SettingsView />);
    const files = await screen.findByRole("textbox", { name: "Hermes log files" });
    fireEvent.change(files, {
      target: { value: "/root/logs/gateway.log, /root/logs/tool_calls.log" },
    });
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(fn).toHaveBeenCalledWith("update_settings", {
      config: {
        ...cfg,
        hermes: {
          ...cfg.hermes,
          logFiles: ["/root/logs/gateway.log", "/root/logs/tool_calls.log"],
        },
      },
    });
  });

  it("selects Control API and previews then installs the offered version", async () => {
    useDevices.getState().setDevices([device()]);
    const fn = mockIpc({
      get_settings: () => cfg,
      preview_control_api_install: () => ({
        installedVersion: null,
        targetVersion: "0.1.0",
        filesToUpdate: ["src/hermes_control/server.py"],
      }),
      install_control_api: () => ({ stdout: "installed", stderr: "", exitCode: 0, durationMs: 1 }),
      test_control_api: () => "0.1.0",
    });
    render(<SettingsView />);
    await userEvent.selectOptions(await screen.findByLabelText("Hermes transport"), "controlApi");
    expect(screen.getByLabelText("Fallback to Termux SSH")).not.toBeChecked();
    await userEvent.click(screen.getByLabelText("Fallback to Termux SSH"));
    expect(useSettings.getState().draft?.hermes.fallbackToSsh).toBe(true);
    await userEvent.click(screen.getByRole("button", { name: "Review install" }));
    expect(await screen.findByText("Version: Not installed to 0.1.0")).toBeInTheDocument();
    expect(screen.getByText("src/hermes_control/server.py")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Install" }));
    await waitFor(() =>
      expect(fn).toHaveBeenCalledWith("install_control_api", { serial: device().serial }),
    );
    await waitFor(() =>
      expect(fn).toHaveBeenCalledWith("test_control_api", { serial: device().serial }),
    );
    expect(useSettings.getState().draft?.hermes.transport).toBe("controlApi");
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

  it("requires confirmation and exports diagnostics with IP redaction enabled", async () => {
    const fn = mockIpc({
      get_settings: () => cfg,
      export_diagnostics: () => "/tmp/hermes-control-center-diagnostics.zip",
    });
    render(<SettingsView />);

    await userEvent.click(await screen.findByRole("button", { name: "Export diagnostics" }));
    expect(
      screen.getByText(/Logs may contain conversation or other private content/),
    ).toBeVisible();
    expect(screen.getByLabelText("Redact IP addresses in diagnostics")).toBeChecked();
    await userEvent.click(screen.getByRole("button", { name: "Export ZIP" }));

    await waitFor(() => expect(fn).toHaveBeenCalledWith("export_diagnostics", { redactIps: true }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
