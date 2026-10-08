import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { parse as parseToml } from "smol-toml";
import { beforeEach, describe, expect, it } from "vitest";
import { resetStores, mockIpc } from "../../test/ipcMock";
import { useTerminal } from "../../stores/terminal";
import { useRoute } from "../../stores/route";
import type {
  ProvisionEvent,
  ProvisionPlan,
  ProvisionRecipe,
  ProvisionStepId,
  ProvisionStepState,
} from "../../types";
import { ProvisioningWizard } from "./ProvisioningWizard";

const recipe: ProvisionRecipe = {
  id: "debian-official",
  name: "Debian + Hermes",
  termuxSource: "fdroid",
  distro: "debian",
  minimumFreeGib: 2,
  minimumTermuxVersion: "0.118.0",
  termuxPackages: ["openssh", "termux-services"],
  distroPackages: ["curl", "ca-certificates"],
  hermesInstall: {
    scriptUrl: "https://hermes-agent.nousresearch.com/install.sh",
    interactive: true,
    nativeAptPackage: null,
    aptKeyFingerprint: null,
  },
  hermesConfigure: { steps: ["hermes setup", "hermes gateway setup"] },
  hermesRuntime: {
    startMode: "supervised",
    gatewayCommand: "hermes gateway run",
    processMatch: "hermes-agent/venv/bin/python",
    statusCommands: ["hermes gateway status"],
    versionCommand: "hermes --version",
    doctorCommand: "hermes doctor",
    updateCommand: "hermes update",
    logFiles: ["/root/.hermes/logs/gateway.log"],
    stateFile: "/root/.hermes/gateway_state.json",
    pathPrepend: ["/root/.local/bin"],
  },
  autostart: true,
  experimental: false,
};

const plan: ProvisionPlan = {
  serial: "192.0.2.10:5555",
  deviceId: "device-id",
  recipeId: recipe.id,
  currentStep: null,
  steps: [
    {
      id: "preflight",
      title: "Preflight",
      state: "done",
      detail: null,
      phoneAction: null,
      consent: null,
    },
    {
      id: "installTermux",
      title: "Install Termux",
      state: "todo",
      detail: null,
      phoneAction: null,
      consent: {
        title: "Install Termux from F-Droid",
        commands: ["Verify SHA-256", "adb install -r termux.apk"],
        destructive: false,
      },
    },
  ],
};

const recipeSource = `id = "debian-official"
name = "Debian + Hermes"
termux_source = "fdroid"
distro = "debian"
minimum_free_gib = 2
termux_packages = ["openssh", "termux-services"]
distro_packages = ["curl", "ca-certificates"]
autostart = true

[hermes_install]
script_url = "https://hermes-agent.nousresearch.com/install.sh"
interactive = true

[hermes_configure]
steps = ["hermes setup", "hermes gateway setup"]

[hermes_runtime]
start_mode = "supervised"
gateway_command = "hermes gateway run"
process_match = "hermes-agent/venv/bin/python"
status_commands = ["hermes gateway status"]
version_command = "hermes --version"
doctor_command = "hermes doctor"
update_command = "hermes update"
log_files = ["/root/.hermes/logs/gateway.log"]
state_file = "/root/.hermes/gateway_state.json"
path_prepend = ["/root/.local/bin"]
`;

describe("ProvisioningWizard", () => {
  beforeEach(() => {
    resetStores();
    useTerminal.setState({ pendingInteractiveLaunch: null });
  });

  it("shows step consent before running and resumes only the approved step", async () => {
    let runCount = 0;
    const invoke = mockIpc({
      list_provision_recipes: () => [recipe],
      get_provision_plan: () => plan,
      run_provision: (args) => {
        runCount += 1;
        const channel = args?.onEvent as { onmessage: (event: ProvisionEvent) => void };
        const event: ProvisionEvent =
          runCount === 1
            ? {
                type: "consentRequired",
                step: "installTermux",
                consent: plan.steps[1]?.consent ?? {
                  title: "Fallback consent",
                  commands: [],
                  destructive: false,
                },
              }
            : { type: "stepDone", step: "installTermux" };
        window.setTimeout(() => channel.onmessage(event));
        return `provision-${runCount}`;
      },
    });
    render(<ProvisioningWizard serial={plan.serial} />);

    const installLabel = await screen.findByText("Install Termux: To do");
    const installRow = installLabel.closest("li");
    if (!installRow) throw new Error("Provisioning step row was not rendered.");
    await userEvent.click(within(installRow).getByRole("button", { name: "Run step" }));
    expect(
      await screen.findByRole("dialog", { name: "Install Termux from F-Droid" }),
    ).toBeVisible();
    expect(screen.getByText("adb install -r termux.apk")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith(
      "run_provision",
      expect.objectContaining({ approvedSteps: [], onlyStep: true }),
    );

    await userEvent.click(screen.getByRole("button", { name: "Approve and continue" }));
    await waitFor(() => expect(runCount).toBe(2));
    expect(invoke).toHaveBeenLastCalledWith(
      "run_provision",
      expect.objectContaining({ approvedSteps: ["installTermux"], onlyStep: true }),
    );
    expect(await screen.findByText("Install Termux: Done")).toBeInTheDocument();
  });

  it("renders the phone preflight blocker without offering execution", async () => {
    mockIpc({
      list_provision_recipes: () => [recipe],
      get_provision_plan: () => ({
        ...plan,
        steps: [
          {
            ...plan.steps[0]!,
            state: "blocked",
            detail: "Connect and authorize this phone over Wireless ADB.",
          },
        ],
      }),
    });
    render(<ProvisioningWizard serial={plan.serial} />);

    expect(
      await screen.findByText("Connect and authorize this phone over Wireless ADB."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Run all" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Run step" })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Uninstall incompatible Termux" })).toBeNull();
  });

  it("renders every provisioning step state and disables completed or blocked steps", async () => {
    const stepStates = [
      ["preflight", "done"],
      ["installTermux", "todo"],
      ["androidSettings", "running"],
      ["launchTermux", "phoneActionNeeded"],
      ["bootstrapSsh", "failed"],
      ["connectSsh", "blocked"],
    ] as const satisfies ReadonlyArray<readonly [ProvisionStepId, ProvisionStepState]>;
    const labels: Record<ProvisionStepState, string> = {
      done: "Done",
      todo: "To do",
      running: "Running",
      phoneActionNeeded: "Phone action",
      failed: "Failed",
      blocked: "Blocked",
    };
    const statePlan: ProvisionPlan = {
      ...plan,
      steps: stepStates.map(([id, state]) => ({
        ...plan.steps[0]!,
        id,
        title: `Step ${state}`,
        state,
        detail: null,
        phoneAction: null,
        consent: null,
      })),
    };
    mockIpc({
      list_provision_recipes: () => [recipe],
      get_provision_plan: () => statePlan,
    });
    render(<ProvisioningWizard serial={plan.serial} />);

    for (const [, state] of stepStates) {
      const status = await screen.findByText(`Step ${state}: ${labels[state]}`);
      const row = status.closest("li");
      if (!row) throw new Error(`Step ${state} row was not rendered.`);
      const runButton = within(row).getByRole("button", { name: "Run step" });
      if (state === "done" || state === "blocked") expect(runButton).toBeDisabled();
      else expect(runButton).toBeEnabled();
    }
  });

  it("resumes a step after a phone-action pause", async () => {
    let runCount = 0;
    const configurePlan: ProvisionPlan = {
      ...plan,
      steps: [
        plan.steps[0]!,
        {
          id: "configureHermes",
          title: "Configure Hermes",
          state: "todo",
          detail: null,
          phoneAction: null,
          consent: null,
        },
      ],
    };
    const invoke = mockIpc({
      list_provision_recipes: () => [recipe],
      get_provision_plan: () => configurePlan,
      run_provision: (args) => {
        runCount += 1;
        const channel = args?.onEvent as { onmessage: (event: ProvisionEvent) => void };
        const event: ProvisionEvent =
          runCount === 1
            ? {
                type: "phoneActionNeeded",
                step: "configureHermes",
                message: "Finish setup in the interactive terminal, then resume.",
              }
            : { type: "stepDone", step: "configureHermes" };
        window.setTimeout(() => channel.onmessage(event));
        return `provision-${runCount}`;
      },
    });
    render(<ProvisioningWizard serial={plan.serial} />);

    const configureLabel = await screen.findByText("Configure Hermes: To do");
    const configureRow = configureLabel.closest("li");
    if (!configureRow) throw new Error("Configure Hermes step was not rendered.");
    await userEvent.click(within(configureRow).getByRole("button", { name: "Run step" }));
    expect(
      await screen.findAllByText("Finish setup in the interactive terminal, then resume."),
    ).toHaveLength(2);
    await userEvent.click(screen.getByRole("button", { name: "Resume" }));

    expect(await screen.findByText("Configure Hermes: Done")).toBeInTheDocument();
    expect(runCount).toBe(2);
    expect(invoke).toHaveBeenLastCalledWith(
      "run_provision",
      expect.objectContaining({ fromStep: "configureHermes", onlyStep: true }),
    );
  });

  it("requires typed confirmation to uninstall incompatible Termux and refreshes preflight", async () => {
    let planRequests = 0;
    const blockedPlan: ProvisionPlan = {
      ...plan,
      steps: [
        {
          ...plan.steps[0]!,
          state: "blocked",
          detail:
            "Installed Termux source (PlayStore) does not match recipe source (Fdroid); replacing Termux deletes its data.",
        },
        ...plan.steps.slice(1),
      ],
    };
    const invoke = mockIpc({
      list_provision_recipes: () => [recipe],
      get_provision_plan: () => (++planRequests === 1 ? blockedPlan : plan),
      uninstall_incompatible_termux: () => undefined,
    });
    render(<ProvisioningWizard serial={plan.serial} />);

    await userEvent.click(
      await screen.findByRole("button", { name: "Uninstall incompatible Termux" }),
    );
    expect(
      screen.getByRole("dialog", { name: `Uninstall Termux from ${plan.serial}?` }),
    ).toBeVisible();
    expect(screen.getByText(/deletes all of its data/)).toBeInTheDocument();
    const uninstall = screen.getByRole("button", { name: "Uninstall Termux" });
    expect(uninstall).toBeDisabled();
    expect(invoke).not.toHaveBeenCalledWith("uninstall_incompatible_termux", expect.anything());

    await userEvent.type(
      screen.getByRole("textbox", { name: "Type UNINSTALL TERMUX to confirm" }),
      "UNINSTALL TERMUX",
    );
    expect(uninstall).toBeEnabled();
    await userEvent.click(uninstall);

    await waitFor(() => expect(planRequests).toBe(2));
    expect(invoke).toHaveBeenCalledWith("uninstall_incompatible_termux", {
      serial: plan.serial,
      recipeId: recipe.id,
      confirmation: "UNINSTALL TERMUX",
    });
    expect(await screen.findByText("Preflight: Done")).toBeInTheDocument();
  });

  it("reviews the installer before requiring explicit approval to run it", async () => {
    const installPlan: ProvisionPlan = {
      ...plan,
      steps: [
        plan.steps[0]!,
        {
          id: "installHermes",
          title: "Install Hermes Agent",
          state: "todo",
          detail: null,
          phoneAction: null,
          consent: null,
        },
      ],
    };
    mockIpc({
      list_provision_recipes: () => [recipe],
      get_provision_plan: () => installPlan,
      run_provision: (args) => {
        const channel = args?.onEvent as { onmessage: (event: ProvisionEvent) => void };
        window.setTimeout(() =>
          channel.onmessage({
            type: "phoneActionNeeded",
            step: "installHermes",
            message: "Review the downloaded installer before running it.",
          }),
        );
        return "provision-install-hermes";
      },
    });
    render(<ProvisioningWizard serial={plan.serial} />);

    const installLabel = await screen.findByText("Install Hermes Agent: To do");
    const installRow = installLabel.closest("li");
    if (!installRow) throw new Error("Install Hermes step was not rendered.");
    await userEvent.click(within(installRow).getByRole("button", { name: "Run step" }));
    await waitFor(() =>
      expect(
        screen.getAllByText("Review the downloaded installer before running it."),
      ).toHaveLength(2),
    );

    await userEvent.click(screen.getByRole("button", { name: "View installer script" }));
    const reviewCommand = useTerminal.getState().pendingInteractiveLaunch?.command ?? "";
    expect(reviewCommand).toContain("wc -c /tmp/hacc-hermes-install.sh");
    expect(reviewCommand).toContain("sha256sum /tmp/hacc-hermes-install.sh");
    expect(reviewCommand).toContain("cat /tmp/hacc-hermes-install.sh");
    expect(reviewCommand).not.toContain("bash /tmp/hacc-hermes-install.sh");
    expect(useRoute.getState().route).toBe("terminal");

    await userEvent.click(screen.getByRole("button", { name: "Run reviewed installer" }));
    expect(
      screen.getByRole("dialog", { name: `Run the reviewed Hermes installer on ${plan.serial}?` }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "Run installer" })).toBeVisible();
    expect(useTerminal.getState().pendingInteractiveLaunch?.command).toBe(reviewCommand);

    await userEvent.click(screen.getByRole("button", { name: "Run installer" }));
    const installCommand = useTerminal.getState().pendingInteractiveLaunch?.command ?? "";
    expect(installCommand).toContain("proot-distro login 'debian'");
    expect(installCommand).toContain("bash /tmp/hacc-hermes-install.sh");
  });

  it("shows proot service and gateway safety hints during interactive Hermes setup", async () => {
    const configurePlan: ProvisionPlan = {
      ...plan,
      steps: [
        plan.steps[0]!,
        {
          id: "configureHermes",
          title: "Configure Hermes",
          state: "todo",
          detail: null,
          phoneAction: null,
          consent: null,
        },
      ],
    };
    mockIpc({
      list_provision_recipes: () => [recipe],
      get_provision_plan: () => configurePlan,
      run_provision: (args) => {
        const channel = args?.onEvent as { onmessage: (event: ProvisionEvent) => void };
        window.setTimeout(() =>
          channel.onmessage({
            type: "phoneActionNeeded",
            step: "configureHermes",
            message: "Complete Hermes setup in the interactive terminal.",
          }),
        );
        return "provision-configure-hermes";
      },
    });
    render(<ProvisioningWizard serial={plan.serial} />);

    const configureLabel = await screen.findByText("Configure Hermes: To do");
    const configureRow = configureLabel.closest("li");
    if (!configureRow) throw new Error("Configure Hermes step was not rendered.");
    await userEvent.click(within(configureRow).getByRole("button", { name: "Run step" }));

    expect(await screen.findByRole("note", { name: "Hermes setup guidance" })).toBeVisible();
    expect(screen.getByText(/there is no systemd/)).toBeInTheDocument();
    expect(screen.getByText(/choose No/)).toBeInTheDocument();
    expect(screen.getByText(/allow-listed or pairing-protected/)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Open setup portal" }));
    const portalCommand = useTerminal.getState().pendingInteractiveLaunch?.command ?? "";
    expect(portalCommand).toContain("hermes setup --portal && hermes gateway setup");
    expect(useRoute.getState().route).toBe("terminal");
  });

  it("edits Hermes YAML while preserving unchanged masked secrets", async () => {
    const configurePlan: ProvisionPlan = {
      ...plan,
      steps: [
        plan.steps[0]!,
        {
          id: "configureHermes",
          title: "Configure Hermes",
          state: "todo",
          detail: null,
          phoneAction: null,
          consent: null,
        },
      ],
    };
    const invoke = mockIpc({
      list_provision_recipes: () => [recipe],
      get_provision_plan: () => configurePlan,
      get_hermes_config: () => 'api_key: "original-secret"\nmodel: initial\n',
      save_hermes_config: () => undefined,
    });
    render(<ProvisioningWizard serial={plan.serial} />);

    const configureRow = (await screen.findByText("Configure Hermes: To do")).closest("li");
    if (!configureRow) throw new Error("Configure Hermes step was not rendered.");
    await userEvent.click(within(configureRow).getByRole("button", { name: "Edit Hermes config" }));
    const yaml = await screen.findByRole("textbox", { name: "Hermes config YAML" });
    expect((yaml as HTMLTextAreaElement).value).toContain('api_key: "********"');

    await userEvent.clear(yaml);
    await userEvent.type(yaml, 'api_key: "********"\nmodel: selected\n');
    await userEvent.click(screen.getByRole("button", { name: "Save config" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_hermes_config", expect.anything()),
    );

    const saveCall = invoke.mock.calls.find(([command]) => command === "save_hermes_config");
    const savedSource = (saveCall?.[1] as { source: string } | undefined)?.source ?? "";
    expect(savedSource).toContain("original-secret");
    expect(savedSource).toContain("selected");
    expect(savedSource).not.toContain("********");
    expect(screen.queryByRole("dialog", { name: /Hermes config.yaml/ })).toBeNull();
  });

  it("duplicates a bundled recipe and saves the edited TOML as a user recipe", async () => {
    const invoke = mockIpc({
      list_provision_recipes: () => [recipe],
      get_provision_plan: () => plan,
      get_provision_recipe_source: () => recipeSource,
      save_provision_recipe: () => ({
        ...recipe,
        id: "debian-official-copy",
        name: "Copy of Debian + Hermes",
      }),
    });
    render(<ProvisioningWizard serial={plan.serial} />);

    await userEvent.click(await screen.findByRole("button", { name: "Duplicate" }));
    const rawToml = await screen.findByRole("textbox", { name: "Raw recipe TOML" });
    expect((rawToml as HTMLTextAreaElement).value).toContain('id = "debian-official-copy"');
    const storageThreshold = screen.getByRole("spinbutton", {
      name: "Minimum free storage (GiB)",
    });
    await userEvent.clear(storageThreshold);
    await userEvent.type(storageThreshold, "4");
    await userEvent.clear(screen.getByRole("textbox", { name: "Termux packages" }));
    await userEvent.type(screen.getByRole("textbox", { name: "Termux packages" }), "openssh, git");
    await userEvent.click(screen.getByRole("button", { name: "Save recipe" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "save_provision_recipe",
        expect.objectContaining({ source: expect.stringContaining('id = "debian-official-copy"') }),
      ),
    );
    expect(await screen.findByRole("combobox", { name: "Provisioning recipe" })).toHaveValue(
      "debian-official-copy",
    );
    const saveCall = invoke.mock.calls.find(([command]) => command === "save_provision_recipe");
    const savedSource = (saveCall?.[1] as { source: string } | undefined)?.source;
    expect(savedSource).toBeDefined();
    expect(parseToml(savedSource ?? "")).toMatchObject({
      minimumFreeGib: 4,
      termuxPackages: ["openssh", "git"],
    });
  });
});
