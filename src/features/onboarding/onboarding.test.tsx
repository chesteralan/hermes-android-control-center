import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { deviceKey, useDevices } from "../../stores/devices";
import { useRoute } from "../../stores/route";
import { ONBOARDING_DISMISS_STORAGE, useUi } from "../../stores/ui";
import { device, hermesStatus } from "../../test/fixtures";
import { mockIpc, resetStores } from "../../test/ipcMock";
import { OnboardingView } from "./OnboardingView";

const adbInfo = { path: "/usr/bin/adb", version: "1.0.41", revision: null };
const adbError = {
  kind: "adbNotFound" as const,
  message: "ADB is not installed or could not be found.",
  details: null,
};

const termuxReady = { ready: true, items: [], distros: [] };
const termuxMissing = {
  ready: false,
  items: [
    {
      id: "installed",
      label: "Termux installed",
      status: "failed" as const,
      detail: null,
      hint: "Install Termux on the phone",
    },
  ],
  distros: [],
};

function setAdbOk(): void {
  useDevices.setState({ adb: { info: adbInfo, error: null, checked: true } });
}

function setAdbMissing(): void {
  useDevices.setState({ adb: { info: null, error: adbError, checked: true } });
}

function connectPhone(): void {
  const key = deviceKey(device());
  useDevices.setState({ devices: { [key]: device() }, activeKey: key });
}

beforeEach(() => {
  resetStores();
  setAdbOk();
  useUi.setState({ onboardingOpen: true });
});

describe("OnboardingView", () => {
  it("shows detection errors with retry when adb is unavailable", async () => {
    setAdbMissing();
    mockIpc({ detect_adb: () => adbInfo });
    render(<OnboardingView />);
    expect(screen.getByText(adbError.message)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Detect again" }));
    expect(
      await screen.findByRole("button", { name: "Pair a phone to set it up" }),
    ).toBeInTheDocument();
  });

  it("leaves to settings from a missing-adb error without recording dismissal", async () => {
    setAdbMissing();
    mockIpc({});
    render(<OnboardingView />);
    await userEvent.click(screen.getByRole("button", { name: "Set ADB path" }));
    expect(useUi.getState().onboardingOpen).toBe(false);
    expect(useRoute.getState().route).toBe("settings");
    expect(localStorage.getItem(ONBOARDING_DISMISS_STORAGE)).toBeNull();
  });

  it("opens the pairing dialog from the connect step", async () => {
    mockIpc({});
    render(<OnboardingView />);
    await userEvent.click(screen.getByRole("button", { name: "Pair a phone to set it up" }));
    expect(useUi.getState().pairTab).toBe("qr");
    await userEvent.click(screen.getByRole("button", { name: "Pair with code" }));
    expect(useUi.getState().pairTab).toBe("code");
  });

  it("advances to the setup choices once a phone is connected", async () => {
    mockIpc({});
    render(<OnboardingView />);
    act(() => connectPhone());
    expect(
      await screen.findByRole("button", { name: "Set up this phone automatically" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "This phone is already set up" }),
    ).toBeInTheDocument();
  });

  it("routes to the wizard and records dismissal for automatic setup", async () => {
    mockIpc({});
    connectPhone();
    render(<OnboardingView />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Set up this phone automatically" }),
    );
    expect(useRoute.getState().route).toBe("device");
    expect(useUi.getState().onboardingOpen).toBe(false);
    expect(localStorage.getItem(ONBOARDING_DISMISS_STORAGE)).toBe("1");
  });

  it("finishes the guide when Termux and Hermes checks pass", async () => {
    mockIpc({ check_termux: () => termuxReady, get_hermes_status: () => hermesStatus() });
    connectPhone();
    render(<OnboardingView />);
    await userEvent.click(
      await screen.findByRole("button", { name: "This phone is already set up" }),
    );
    expect(await screen.findByText("Termux is set up")).toBeInTheDocument();
    expect(screen.getByText("Hermes is running")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Finish" }));
    expect(useUi.getState().onboardingOpen).toBe(false);
    expect(localStorage.getItem(ONBOARDING_DISMISS_STORAGE)).toBe("1");
    expect(useRoute.getState().route).toBe("dashboard");
  });

  it("warns and allows finishing when a check fails", async () => {
    mockIpc({
      check_termux: () => termuxMissing,
      get_hermes_status: () =>
        hermesStatus({ gateway: "stopped", gatewayPid: null, uptimeSecs: null }),
    });
    connectPhone();
    render(<OnboardingView />);
    await userEvent.click(
      await screen.findByRole("button", { name: "This phone is already set up" }),
    );
    expect(await screen.findByText("Termux is not ready")).toBeInTheDocument();
    expect(screen.getByText("Hermes is not running")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open setup wizard" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Finish anyway" }));
    expect(useUi.getState().onboardingOpen).toBe(false);
    expect(localStorage.getItem(ONBOARDING_DISMISS_STORAGE)).toBe("1");
  });

  it("surfaces a check error and still allows finishing", async () => {
    mockIpc({
      check_termux: () => termuxReady,
      get_hermes_status: () => {
        throw { kind: "io", message: "SSH unreachable", details: null };
      },
    });
    connectPhone();
    render(<OnboardingView />);
    await userEvent.click(
      await screen.findByRole("button", { name: "This phone is already set up" }),
    );
    expect(await screen.findByText("SSH unreachable")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Finish anyway" }));
    expect(useUi.getState().onboardingOpen).toBe(false);
    expect(localStorage.getItem(ONBOARDING_DISMISS_STORAGE)).toBe("1");
  });

  it("dismisses the guide from any step with Skip for now", async () => {
    mockIpc({});
    render(<OnboardingView />);
    await userEvent.click(screen.getByRole("button", { name: "Skip for now" }));
    expect(useUi.getState().onboardingOpen).toBe(false);
    expect(localStorage.getItem(ONBOARDING_DISMISS_STORAGE)).toBe("1");
  });
});
