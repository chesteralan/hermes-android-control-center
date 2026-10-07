import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { check } from "@tauri-apps/plugin-updater";
import App from "../App";
import { useRoute } from "../stores/route";
import { mockIpc, resetStores } from "../test/ipcMock";

vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn() }));

describe("App shell", () => {
  beforeEach(() => {
    resetStores();
    mockIpc({
      detect_adb: () => ({ path: "/opt/homebrew/bin/adb", version: "1.0.41", revision: null }),
      list_devices: () => [],
    });
  });

  it("navigates between views and marks the active item", async () => {
    render(<App />);
    expect(await screen.findByText("No phone connected")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /^Device/ }));
    expect(useRoute.getState().route).toBe("device");
    expect(screen.getByRole("button", { name: /^Device/ })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: "Connect" })).toBeInTheDocument();
  });

  it("shows the ADB banner when adb is missing", async () => {
    mockIpc({
      detect_adb: () => {
        throw {
          kind: "adbNotFound",
          message: "ADB is not installed or could not be found.",
          details: "Searched:\n/opt/homebrew/bin/adb",
        };
      },
      list_devices: () => {
        throw {
          kind: "adbNotFound",
          message: "ADB is not installed or could not be found.",
          details: null,
        };
      },
    });
    render(<App />);
    expect(
      await screen.findByText("ADB is not installed or could not be found."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Detect again" })).toBeInTheDocument();
  });

  it("opens the command palette, filters views, and navigates by selection", async () => {
    render(<App />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });

    const palette = await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.type(
      within(palette).getByRole("textbox", { name: "Search views and commands" }),
      "term",
    );
    expect(within(palette).getByRole("button", { name: /Terminal/ })).toBeInTheDocument();
    expect(within(palette).queryByRole("button", { name: /^Dashboard/ })).not.toBeInTheDocument();

    await userEvent.click(within(palette).getByRole("button", { name: /Terminal/ }));
    expect(useRoute.getState().route).toBe("terminal");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("shows the synchronized application version in About", async () => {
    render(<App />);
    expect(screen.getByText(`v${__APP_VERSION__}`)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "About" }));
    expect(screen.getByRole("dialog", { name: "About Hermes Control Center" })).toHaveTextContent(
      `v${__APP_VERSION__}`,
    );
  });

  it("does not invoke the updater for Linux package-managed installs", async () => {
    vi.mocked(check).mockClear();
    mockIpc({
      detect_adb: () => ({ path: "/usr/bin/adb", version: "1.0.41", revision: null }),
      list_devices: () => [],
      supports_in_app_updates: () => false,
    });
    render(<App />);
    await userEvent.click(screen.getByRole("button", { name: "About" }));
    await userEvent.click(screen.getByRole("button", { name: "Check for Updates" }));
    expect(await screen.findByText(/managed by your Linux package manager/)).toBeInTheDocument();
    expect(check).not.toHaveBeenCalled();
  });
});
