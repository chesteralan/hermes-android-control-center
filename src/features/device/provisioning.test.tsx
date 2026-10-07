import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { parse as parseToml } from "smol-toml";
import { beforeEach, describe, expect, it } from "vitest";
import { resetStores, mockIpc } from "../../test/ipcMock";
import type { ProvisionEvent, ProvisionPlan, ProvisionRecipe } from "../../types";
import { ProvisioningWizard } from "./ProvisioningWizard";

const recipe: ProvisionRecipe = {
  id: "debian-official",
  name: "Debian + Hermes",
  termuxSource: "fdroid",
  distro: "debian",
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
  beforeEach(() => resetStores());

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
    expect(parseToml(savedSource ?? "")).toMatchObject({ termuxPackages: ["openssh", "git"] });
  });
});
