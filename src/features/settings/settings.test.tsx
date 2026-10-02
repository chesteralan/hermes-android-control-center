import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { mockIpc, resetStores } from "../../test/ipcMock";
import type { AppConfig } from "../../types";
import { SettingsView } from "./SettingsView";

const cfg: AppConfig = {
  version: 1,
  adbPath: null,
  knownAddresses: [],
  reconnect: { enabled: true, scheduleMs: [1000, 2000] },
  hermes: {
    startCommand: "",
    stopCommand: "",
    restartCommand: "",
    statusCommand: "",
    logCommand: "",
    processMatch: "hermes-agent/hermes",
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
    const start = await screen.findByLabelText("Start command");
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
});
