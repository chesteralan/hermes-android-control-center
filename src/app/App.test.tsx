import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "../App";
import { useRoute } from "../stores/route";
import { mockIpc, resetStores } from "../test/ipcMock";

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
});
