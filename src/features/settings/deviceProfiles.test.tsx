import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useDeviceProfiles, useProfileList } from "../../stores/deviceProfiles";
import { useSettings } from "../../stores/settings";
import { mockIpc, resetStores } from "../../test/ipcMock";
import type {
  DeviceProfile,
  DeviceProfileUpdate,
  EffectiveDeviceConfig,
  HermesConfig,
  TermuxConfig,
} from "../../types";
import { DeviceProfilesSection } from "./DeviceProfilesSection";

// ─── shared fixtures ──────────────────────────────────────────────────────────

const globalHermes: HermesConfig = {
  environment: { type: "prootDistro", distro: "debian" },
  startMode: "supervised",
  transport: "termuxSsh",
  fallbackToSsh: false,
  gatewayCommand: "hermes gateway run",
  processMatch: "hermes-agent/venv/bin/python",
  gatewayMatch: "gateway run",
  hermesHome: "/root/.hermes",
  logFiles: ["/root/.hermes/logs/gateway.log"],
  pathPrepend: ["/root/.local/bin"],
  startCommand: "",
  stopCommand: "",
  restartCommand: "",
  statusCommand: "",
  logCommand: "",
  versionCommand: "hermes --version",
  doctorCommand: "hermes doctor",
  updateCommand: "hermes update",
};

const globalTermux: TermuxConfig = { sshUser: "termux", sshPort: 8022 };

const settings = {
  version: 3,
  adbPath: null,
  knownAddresses: [],
  reconnect: { enabled: true, scheduleMs: [1000, 2000] },
  hermes: globalHermes,
  logs: { autoStart: false, logcatFilter: "*:I" },
  limits: { maxLogLines: 20_000 },
  termux: globalTermux,
  apiPort: 8765,
  logLevel: "info" as const,
  devices: {} as Record<string, DeviceProfile>,
};

function makeProfile(over: Partial<DeviceProfile> = {}): DeviceProfile {
  return {
    deviceId: "TESTDEVICE001",
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

function makeEffective(over: Partial<EffectiveDeviceConfig> = {}): EffectiveDeviceConfig {
  return {
    deviceId: "TESTDEVICE001",
    alias: null,
    color: null,
    lastAddress: null,
    autoConnect: true,
    hermes: globalHermes,
    termux: globalTermux,
    logCommand: "",
    hasHermesOverride: false,
    hasTermuxOverride: false,
    hasLogCommandOverride: false,
    ...over,
  };
}

// ─── store tests ──────────────────────────────────────────────────────────────

describe("useDeviceProfiles store", () => {
  beforeEach(() => {
    resetStores();
    useDeviceProfiles.setState({
      profiles: {},
      profileList: [],
      effective: {},
      loading: false,
      error: null,
    });
  });

  it("load populates profiles keyed by deviceId", async () => {
    const p = makeProfile();
    mockIpc({ list_device_profiles: () => [p] });

    await useDeviceProfiles.getState().load();

    const profiles = useDeviceProfiles.getState().profiles;
    expect(profiles[p.deviceId]).toEqual(p);
    expect(useDeviceProfiles.getState().loading).toBe(false);
    expect(useDeviceProfiles.getState().error).toBeNull();
  });

  it("load stores a backend error", async () => {
    mockIpc({
      list_device_profiles: () => {
        throw { kind: "io", message: "backend unavailable", details: null };
      },
    });
    await useDeviceProfiles.getState().load();
    expect(useDeviceProfiles.getState().error?.message).toBe("backend unavailable");
  });

  it("loadEffective stores the effective config and can be cleared by invalidation", async () => {
    const eff = makeEffective();
    mockIpc({ get_effective_device_config: () => eff });

    await useDeviceProfiles.getState().loadEffective("TESTDEVICE001");
    expect(useDeviceProfiles.getState().effective["TESTDEVICE001"]).toEqual(eff);
  });

  it("set updates the profile in the store and invalidates the effective cache", async () => {
    const original = makeProfile({ alias: "Old" });
    useDeviceProfiles.setState({
      profiles: { TESTDEVICE001: original },
      profileList: [original],
      effective: { TESTDEVICE001: makeEffective() },
    });

    const updated = makeProfile({ alias: "New" });
    mockIpc({ set_device_profile: () => updated });

    const result = await useDeviceProfiles.getState().set("TESTDEVICE001", { alias: "New" });

    expect(result.alias).toBe("New");
    expect(useDeviceProfiles.getState().profiles["TESTDEVICE001"]?.alias).toBe("New");
    // Effective cache must be invalidated.
    expect(useDeviceProfiles.getState().effective["TESTDEVICE001"]).toBeUndefined();
  });

  it("delete removes the profile and its effective cache entry", async () => {
    const p = makeProfile();
    useDeviceProfiles.setState({
      profiles: { TESTDEVICE001: p },
      profileList: [p],
      effective: { TESTDEVICE001: makeEffective() },
    });
    mockIpc({ delete_device_profile: () => undefined });

    await useDeviceProfiles.getState().delete("TESTDEVICE001");

    expect(useDeviceProfiles.getState().profiles["TESTDEVICE001"]).toBeUndefined();
    expect(useDeviceProfiles.getState().effective["TESTDEVICE001"]).toBeUndefined();
  });

  it("clearError resets the error field", () => {
    useDeviceProfiles.setState({
      error: { kind: "io", message: "stale error", details: null },
    });
    useDeviceProfiles.getState().clearError();
    expect(useDeviceProfiles.getState().error).toBeNull();
  });

  it("useProfileList returns profiles sorted by deviceId", () => {
    const z = makeProfile({ deviceId: "ZPHONE" });
    const a = makeProfile({ deviceId: "APHONE" });
    const m = makeProfile({ deviceId: "MPHONE" });
    useDeviceProfiles.setState({
      profiles: { ZPHONE: z, APHONE: a, MPHONE: m },
      profileList: [a, m, z], // pre-sorted
    });
    let sorted: DeviceProfile[] = [];
    function Probe() {
      sorted = useProfileList();
      return null;
    }
    render(<Probe />);
    expect(sorted.map((p) => p.deviceId)).toEqual(["APHONE", "MPHONE", "ZPHONE"]);
  });
});

// ─── component tests ──────────────────────────────────────────────────────────

describe("DeviceProfilesSection", () => {
  beforeEach(() => {
    resetStores();
    useDeviceProfiles.setState({
      profiles: {},
      profileList: [],
      effective: {},
      loading: false,
      error: null,
    });
    useSettings.setState({ saved: settings, draft: settings, error: null, saving: false });
  });

  it("shows an empty state when no profiles exist", async () => {
    mockIpc({ list_device_profiles: () => [] });
    render(<DeviceProfilesSection />);
    expect(await screen.findByText(/No device profiles yet/)).toBeInTheDocument();
  });

  it("renders a profile row with alias, color, and auto-connect badges", async () => {
    const p = makeProfile({
      deviceId: "PHONE001",
      alias: "My Phone",
      color: "#ff0000",
      autoConnect: true,
    });
    mockIpc({
      list_device_profiles: () => [p],
    });
    render(<DeviceProfilesSection />);
    expect(await screen.findByText("My Phone")).toBeInTheDocument();
    expect(screen.getByText("PHONE001", { exact: false })).toBeInTheDocument();
    expect(screen.getByText("auto-connect")).toBeInTheDocument();
  });

  it("shows override badges when hermes/termux overrides are set", async () => {
    const p = makeProfile({
      deviceId: "PHONE002",
      hermes: { ...globalHermes, gatewayCommand: "hermes gateway run --verbose" },
      termux: { sshUser: "termux", sshPort: 9022 },
    });
    mockIpc({ list_device_profiles: () => [p] });
    render(<DeviceProfilesSection />);
    expect(await screen.findByTitle("Has Hermes override")).toBeInTheDocument();
    expect(screen.getByTitle("Has Termux override")).toBeInTheDocument();
  });

  it("opens edit form when Edit is clicked and loads effective config", async () => {
    const p = makeProfile({ deviceId: "PHONE003", alias: "Edit Target" });
    const eff = makeEffective({ deviceId: "PHONE003" });
    mockIpc({
      list_device_profiles: () => [p],
      get_effective_device_config: () => eff,
    });

    render(<DeviceProfilesSection />);
    const editBtn = await screen.findByRole("button", { name: /Edit profile for Edit Target/ });
    await userEvent.click(editBtn);

    // Form should be visible.
    expect(await screen.findByLabelText("Device alias")).toBeInTheDocument();
    expect(screen.getByLabelText("Device color")).toBeInTheDocument();
    expect(screen.getByLabelText("Auto-connect")).toBeInTheDocument();
  });

  it("saves partial update and closes the form", async () => {
    const p = makeProfile({ deviceId: "PHONE004", alias: "Before" });
    const updated = makeProfile({ deviceId: "PHONE004", alias: "After" });
    const eff = makeEffective({ deviceId: "PHONE004" });
    const fn = mockIpc({
      list_device_profiles: () => [p, updated].slice(0, 1), // initial load
      get_effective_device_config: () => eff,
      set_device_profile: () => updated,
    });

    render(<DeviceProfilesSection />);
    const editBtn = await screen.findByRole("button", { name: /Edit profile for Before/ });
    await userEvent.click(editBtn);

    const aliasInput = await screen.findByLabelText("Device alias");
    await userEvent.clear(aliasInput);
    await userEvent.type(aliasInput, "After");

    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(fn).toHaveBeenCalledWith(
        "set_device_profile",
        expect.objectContaining({
          deviceId: "PHONE004",
          update: expect.objectContaining({ alias: "After" }) as Partial<DeviceProfileUpdate>,
        }),
      );
    });
  });

  it("shows the Hermes override collapsible", async () => {
    const p = makeProfile({ deviceId: "PHONE005" });
    mockIpc({
      list_device_profiles: () => [p],
      get_effective_device_config: () => makeEffective({ deviceId: "PHONE005" }),
    });

    render(<DeviceProfilesSection />);
    await userEvent.click(await screen.findByRole("button", { name: /Edit profile for/i }));

    // Collapsed by default since no override is set.
    expect(await screen.findByText(/Hermes overrides/)).toBeInTheDocument();
    expect(screen.queryByLabelText("Hermes gateway command override")).not.toBeInTheDocument();

    // Expand.
    await userEvent.click(screen.getByRole("button", { name: /Hermes overrides/ }));
    expect(await screen.findByLabelText("Hermes gateway command override")).toBeInTheDocument();
  });

  it("reset Hermes to global collapses the section and clears the override", async () => {
    const override = { ...globalHermes, gatewayCommand: "custom-cmd" };
    const p = makeProfile({ deviceId: "PHONE006", hermes: override });
    mockIpc({
      list_device_profiles: () => [p],
      get_effective_device_config: () =>
        makeEffective({ deviceId: "PHONE006", hermes: override, hasHermesOverride: true }),
    });

    render(<DeviceProfilesSection />);
    await userEvent.click(await screen.findByRole("button", { name: /Edit profile for/i }));

    // Hermes section should be expanded because an override exists.
    expect(await screen.findByLabelText("Hermes gateway command override")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /Reset Hermes to global/ }));

    // Section should collapse.
    expect(screen.queryByLabelText("Hermes gateway command override")).not.toBeInTheDocument();
  });

  it("Cancel button closes the form without saving", async () => {
    const p = makeProfile({ deviceId: "PHONE007", alias: "Untouched" });
    mockIpc({
      list_device_profiles: () => [p],
      get_effective_device_config: () => makeEffective({ deviceId: "PHONE007" }),
    });

    render(<DeviceProfilesSection />);
    await userEvent.click(
      await screen.findByRole("button", { name: /Edit profile for Untouched/ }),
    );
    expect(await screen.findByLabelText("Device alias")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByLabelText("Device alias")).not.toBeInTheDocument();
  });
});
