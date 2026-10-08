import { useEffect, useState } from "react";
import { parse as parseToml, stringify as stringifyToml } from "smol-toml";
import { Button } from "../../components/Button";
import { Card } from "../../components/Card";
import { ConfirmDialog } from "../../components/ConfirmDialog";
import { Dialog } from "../../components/Dialog";
import { ErrorPanel } from "../../components/ErrorPanel";
import { StatusDot } from "../../components/StatusDot";
import { ipc, toErrorPayload } from "../../lib/ipc";
import { useRoute } from "../../stores/route";
import { useTerminal } from "../../stores/terminal";
import type {
  ErrorPayload,
  ProvisionConsent,
  ProvisionEvent,
  ProvisionPlan,
  ProvisionRecipe,
  ProvisionStepId,
  ProvisionStepState,
} from "../../types";

interface ProvisioningWizardProps {
  serial: string;
}

interface ConsentPrompt {
  step: ProvisionStepId;
  consent: ProvisionConsent;
  onlyStep: boolean;
}

interface PhoneAction {
  step: ProvisionStepId;
  message: string;
  onlyStep: boolean;
}

const TERMUX_UNINSTALL_CONFIRMATION = "UNINSTALL TERMUX";

const stepLabel: Record<ProvisionStepState, string> = {
  done: "Done",
  todo: "To do",
  running: "Running",
  phoneActionNeeded: "Phone action",
  failed: "Failed",
  blocked: "Blocked",
};

function stateTone(state: ProvisionStepState): "success" | "warning" | "danger" | "muted" {
  if (state === "done") return "success";
  if (state === "running") return "warning";
  if (state === "failed" || state === "blocked") return "danger";
  return "muted";
}

function camelizeToml(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(camelizeToml);
  if (typeof value !== "object" || value === null) return value;
  return Object.fromEntries(
    Object.entries(value).map(([key, nested]) => [
      key.replace(/_([a-z])/g, (_, letter: string) => letter.toUpperCase()),
      camelizeToml(nested),
    ]),
  );
}

function parseRecipeDraft(source: string): Record<string, unknown> | null {
  try {
    const parsed = camelizeToml(parseToml(source));
    return typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

function isTermuxSourceMismatch(detail: string | null): boolean {
  return detail?.startsWith("Installed Termux source (") === true;
}

function setRecipeDraftValue(source: string, path: string[], value: unknown): string | null {
  const draft = parseRecipeDraft(source);
  if (!draft) return null;
  let current = draft;
  for (const key of path.slice(0, -1)) {
    const nested = current[key];
    if (typeof nested !== "object" || nested === null || Array.isArray(nested)) return null;
    current = nested as Record<string, unknown>;
  }
  const lastKey = path.at(-1);
  if (!lastKey) return null;
  if (value === "") delete current[lastKey];
  else current[lastKey] = value;
  try {
    return stringifyToml(draft);
  } catch {
    return null;
  }
}

function shellQuote(value: string): string {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}

function interactiveCommandForStep(
  recipe: ProvisionRecipe | undefined,
  step: ProvisionStepId,
  usePortal = false,
): string | null {
  if (!recipe) return null;
  if (step === "installHermes" && recipe.hermesInstall.scriptUrl) {
    const url = shellQuote(recipe.hermesInstall.scriptUrl);
    const review = `curl -fsSL ${url} -o /tmp/hacc-hermes-install.sh && wc -c /tmp/hacc-hermes-install.sh && sha256sum /tmp/hacc-hermes-install.sh && cat /tmp/hacc-hermes-install.sh`;
    return recipe.distro
      ? `proot-distro login ${shellQuote(recipe.distro)} -- bash -lc ${shellQuote(review)}`
      : review;
  }
  if (step === "configureHermes") {
    const commands = [...recipe.hermesConfigure.steps];
    if (usePortal) {
      if (commands[0]?.trim() !== "hermes setup") return null;
      commands[0] = "hermes setup --portal";
    }
    const command = commands.join(" && ");
    return recipe.distro
      ? `proot-distro login ${shellQuote(recipe.distro)} -- bash -lc ${shellQuote(command)}`
      : command;
  }
  return null;
}

function reviewedInstallerCommand(recipe: ProvisionRecipe | undefined): string | null {
  if (!recipe?.hermesInstall.scriptUrl) return null;
  const command = "bash /tmp/hacc-hermes-install.sh";
  return recipe.distro
    ? `proot-distro login ${shellQuote(recipe.distro)} -- bash -lc ${shellQuote(command)}`
    : command;
}

export function ProvisioningWizard({ serial }: ProvisioningWizardProps) {
  const go = useRoute((state) => state.go);
  const requestInteractiveLaunch = useTerminal((state) => state.requestInteractiveLaunch);
  const [recipes, setRecipes] = useState<ProvisionRecipe[]>([]);
  const [recipeId, setRecipeId] = useState("");
  const [plan, setPlan] = useState<ProvisionPlan | null>(null);
  const [starting, setStarting] = useState(false);
  const [runId, setRunId] = useState<string | null>(null);
  const [consentPrompt, setConsentPrompt] = useState<ConsentPrompt | null>(null);
  const [installScriptApprovalOpen, setInstallScriptApprovalOpen] = useState(false);
  const [phoneAction, setPhoneAction] = useState<PhoneAction | null>(null);
  const [resetPromptOpen, setResetPromptOpen] = useState(false);
  const [termuxUninstallOpen, setTermuxUninstallOpen] = useState(false);
  const [termuxUninstallConfirmation, setTermuxUninstallConfirmation] = useState("");
  const [uninstallingTermux, setUninstallingTermux] = useState(false);
  const [recipeEditorOpen, setRecipeEditorOpen] = useState(false);
  const [recipeEditorSource, setRecipeEditorSource] = useState("");
  const [recipeEditorBusy, setRecipeEditorBusy] = useState(false);
  const [error, setError] = useState<ErrorPayload | null>(null);
  const [output, setOutput] = useState<string[]>([]);
  const loading = Boolean(
    recipeId && (!plan || plan.recipeId !== recipeId || plan.serial !== serial) && !error,
  );
  const busy = loading || starting || runId !== null || uninstallingTermux;

  useEffect(() => {
    let active = true;
    void ipc
      .listProvisionRecipes()
      .then((list) => {
        if (!active) return;
        setRecipes(list);
        setRecipeId((current) => current || list[0]?.id || "");
      })
      .catch((cause: unknown) => active && setError(toErrorPayload(cause)));
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (!recipeId) return;
    let active = true;
    void ipc
      .getProvisionPlan(serial, recipeId)
      .then((nextPlan) => active && setPlan(nextPlan))
      .catch((cause: unknown) => active && setError(toErrorPayload(cause)));
    return () => {
      active = false;
    };
  }, [recipeId, serial]);

  function updateStep(
    step: ProvisionStepId,
    state: ProvisionStepState,
    detail: string | null,
  ): void {
    setPlan((current) =>
      current
        ? {
            ...current,
            steps: current.steps.map((item) =>
              item.id === step ? { ...item, state, detail } : item,
            ),
          }
        : current,
    );
  }

  function handleEvent(event: ProvisionEvent, onlyStep: boolean): void {
    switch (event.type) {
      case "stepStarted":
        setPhoneAction(null);
        updateStep(event.step, "running", null);
        break;
      case "output":
        {
          const streamEvent = event.event;
          switch (streamEvent.type) {
            case "stdout":
            case "stderr":
              setOutput((current) => [...current.slice(-399), streamEvent.line]);
              break;
            case "error":
              setOutput((current) => [...current.slice(-399), streamEvent.error.message]);
              break;
            case "exit":
              break;
          }
        }
        break;
      case "consentRequired":
        setStarting(false);
        setRunId(null);
        setConsentPrompt({ step: event.step, consent: event.consent, onlyStep });
        break;
      case "phoneActionNeeded":
        setStarting(false);
        setRunId(null);
        setPhoneAction({ step: event.step, message: event.message, onlyStep });
        updateStep(event.step, "phoneActionNeeded", event.message);
        break;
      case "stepBlocked":
        setStarting(false);
        setRunId(null);
        updateStep(event.step, "blocked", event.reason);
        break;
      case "stepDone":
        updateStep(event.step, "done", null);
        if (onlyStep) {
          setStarting(false);
          setRunId(null);
        }
        break;
      case "stepFailed":
        setStarting(false);
        setRunId(null);
        updateStep(event.step, "failed", event.error.message);
        setError(event.error);
        break;
      case "planDone":
        setStarting(false);
        setRunId(null);
        setPhoneAction(null);
        void ipc.getProvisionPlan(serial, recipeId).then(setPlan).catch(setError);
        break;
    }
  }

  async function beginRun(
    fromStep: ProvisionStepId | null = null,
    onlyStep = false,
    approvedSteps: ProvisionStepId[] = [],
  ): Promise<void> {
    if (busy || !recipeId) return;
    setStarting(true);
    setError(null);
    setOutput([]);
    let runFinishedBeforeResponse = false;
    try {
      const id = await ipc.runProvision(
        serial,
        recipeId,
        fromStep,
        onlyStep,
        approvedSteps,
        (event) => {
          if (
            event.type === "consentRequired" ||
            event.type === "phoneActionNeeded" ||
            event.type === "stepBlocked" ||
            event.type === "stepFailed" ||
            event.type === "planDone" ||
            (event.type === "stepDone" && onlyStep)
          ) {
            runFinishedBeforeResponse = true;
          }
          handleEvent(event, onlyStep);
        },
      );
      if (!runFinishedBeforeResponse) setRunId(id);
      setStarting(false);
    } catch (cause) {
      setStarting(false);
      setError(toErrorPayload(cause));
    }
  }

  async function cancelRun(): Promise<void> {
    if (!runId) return;
    try {
      await ipc.cancelProvision(runId);
    } catch (cause) {
      setError(toErrorPayload(cause));
    } finally {
      setRunId(null);
      setStarting(false);
    }
  }

  async function resetProgress(): Promise<void> {
    try {
      await ipc.resetProvisionProgress(serial, recipeId);
      setResetPromptOpen(false);
      setPhoneAction(null);
      setOutput([]);
      setPlan(await ipc.getProvisionPlan(serial, recipeId));
    } catch (cause) {
      setError(toErrorPayload(cause));
    }
  }

  async function uninstallIncompatibleTermux(): Promise<void> {
    if (termuxUninstallConfirmation !== TERMUX_UNINSTALL_CONFIRMATION) return;
    setUninstallingTermux(true);
    setError(null);
    try {
      await ipc.uninstallIncompatibleTermux(serial, recipeId, termuxUninstallConfirmation);
      setTermuxUninstallOpen(false);
      setTermuxUninstallConfirmation("");
      setPhoneAction(null);
      setOutput([]);
      setPlan(await ipc.getProvisionPlan(serial, recipeId));
    } catch (cause) {
      setError(toErrorPayload(cause));
    } finally {
      setUninstallingTermux(false);
    }
  }

  async function openRecipeEditor(duplicate: boolean): Promise<void> {
    if (!recipeId) return;
    setRecipeEditorBusy(true);
    setError(null);
    try {
      const source = await ipc.getProvisionRecipeSource(recipeId);
      const draft = parseRecipeDraft(source);
      if (!draft) throw new Error("Recipe source could not be parsed as TOML.");
      if (duplicate) {
        setRecipeEditorSource(
          stringifyToml({
            ...draft,
            id: `${String(draft.id)}-copy`,
            name: `Copy of ${String(draft.name)}`,
          }),
        );
      } else {
        setRecipeEditorSource(source);
      }
      setRecipeEditorOpen(true);
    } catch (cause) {
      setError(toErrorPayload(cause));
    } finally {
      setRecipeEditorBusy(false);
    }
  }

  async function saveRecipeEditor(): Promise<void> {
    setRecipeEditorBusy(true);
    setError(null);
    try {
      const recipe = await ipc.saveProvisionRecipe(recipeEditorSource);
      setRecipes((current) => [...current.filter((item) => item.id !== recipe.id), recipe]);
      setRecipeId(recipe.id);
      setRecipeEditorOpen(false);
    } catch (cause) {
      setError(toErrorPayload(cause));
    } finally {
      setRecipeEditorBusy(false);
    }
  }

  async function importRecipe(): Promise<void> {
    setError(null);
    try {
      const recipe = await ipc.importProvisionRecipe();
      if (!recipe) return;
      setRecipes((current) => [...current.filter((item) => item.id !== recipe.id), recipe]);
      setRecipeId(recipe.id);
    } catch (cause) {
      setError(toErrorPayload(cause));
    }
  }

  async function exportRecipe(): Promise<void> {
    try {
      await ipc.exportProvisionRecipe(recipeId);
    } catch (cause) {
      setError(toErrorPayload(cause));
    }
  }

  function openInteractiveStep(step: ProvisionStepId, usePortal = false): void {
    const command = interactiveCommandForStep(activeRecipe, step, usePortal);
    if (!command) return;
    requestInteractiveLaunch(serial, command);
    go("terminal");
  }

  function runReviewedInstaller(): void {
    const command = reviewedInstallerCommand(activeRecipe);
    if (!command) return;
    setInstallScriptApprovalOpen(false);
    requestInteractiveLaunch(serial, command);
    go("terminal");
  }

  const activeRecipe = recipes.find((recipe) => recipe.id === recipeId);
  return (
    <>
      <Card
        title="Set up this phone"
        actions={
          <div className="flex gap-2">
            {runId ? (
              <Button variant="danger" onClick={() => void cancelRun()}>
                Cancel
              </Button>
            ) : (
              <Button
                variant="primary"
                loading={starting}
                disabled={!plan || busy || plan.steps.some((step) => step.state === "blocked")}
                onClick={() => void beginRun()}
              >
                Run all
              </Button>
            )}
          </div>
        }
      >
        <div className="mb-4 flex flex-wrap items-center gap-3">
          <label className="flex items-center gap-2">
            <span className="text-muted">Recipe</span>
            <select
              aria-label="Provisioning recipe"
              className="rounded-md border border-border bg-bg px-2 py-1.5"
              value={recipeId}
              disabled={busy}
              onChange={(event) => {
                setPlan(null);
                setError(null);
                setRecipeId(event.target.value);
                setPhoneAction(null);
                setOutput([]);
              }}
            >
              {recipes.map((recipe) => (
                <option key={recipe.id} value={recipe.id}>
                  {recipe.name}
                  {recipe.experimental ? " (experimental)" : ""}
                </option>
              ))}
            </select>
          </label>
          <Button disabled={!plan || busy} onClick={() => setResetPromptOpen(true)}>
            Reset progress
          </Button>
          <Button disabled={busy || recipeEditorBusy} onClick={() => void openRecipeEditor(false)}>
            Edit TOML
          </Button>
          <Button disabled={busy || recipeEditorBusy} onClick={() => void openRecipeEditor(true)}>
            Duplicate
          </Button>
          <Button disabled={busy} onClick={() => void importRecipe()}>
            Import TOML
          </Button>
          <Button disabled={busy || !recipeId} onClick={() => void exportRecipe()}>
            Export TOML
          </Button>
          {activeRecipe && (
            <span className="text-muted">{activeRecipe.distro || "Native Termux"}</span>
          )}
        </div>

        {loading && (
          <p role="status" className="text-muted">
            Checking this phone…
          </p>
        )}
        {plan && (
          <ol className="divide-y divide-border" aria-label="Provisioning steps">
            {plan.steps.map((step) => (
              <li key={step.id} className="flex flex-wrap items-center gap-3 py-2.5">
                <StatusDot
                  tone={stateTone(step.state)}
                  label={`${step.title}: ${stepLabel[step.state]}`}
                />
                {step.detail && <span className="text-warning">{step.detail}</span>}
                {step.id === "preflight" &&
                  step.state === "blocked" &&
                  isTermuxSourceMismatch(step.detail) && (
                    <Button
                      variant="danger"
                      disabled={busy}
                      onClick={() => {
                        setTermuxUninstallConfirmation("");
                        setTermuxUninstallOpen(true);
                      }}
                    >
                      Uninstall incompatible Termux
                    </Button>
                  )}
                {phoneAction?.step === step.id && (
                  <span role="status" className="w-full pl-4 text-warning">
                    {phoneAction.message}
                  </span>
                )}
                {phoneAction?.step === "configureHermes" && step.id === "configureHermes" && (
                  <aside
                    role="note"
                    aria-label="Hermes setup guidance"
                    className="w-full rounded-md border border-warning/30 bg-warning/5 p-3 text-[12px]"
                  >
                    <p>
                      In proot-distro there is no systemd. If setup asks to install or start a
                      gateway service, choose No; this app supervises the gateway.
                    </p>
                    <p className="mt-2 text-muted">
                      Keep the gateway restricted to users you trust; direct messages should remain
                      allow-listed or pairing-protected.
                    </p>
                  </aside>
                )}
                <span className="ml-auto flex gap-2">
                  {phoneAction?.step === step.id ? (
                    <>
                      <Button
                        disabled={busy}
                        onClick={() => void beginRun(step.id, phoneAction.onlyStep)}
                      >
                        Resume
                      </Button>
                      {interactiveCommandForStep(activeRecipe, step.id) && (
                        <Button
                          variant="ghost"
                          disabled={busy}
                          onClick={() => openInteractiveStep(step.id)}
                        >
                          {step.id === "installHermes"
                            ? "View installer script"
                            : "Open interactive step"}
                        </Button>
                      )}
                      {step.id === "configureHermes" &&
                        interactiveCommandForStep(activeRecipe, step.id, true) && (
                          <Button
                            variant="ghost"
                            disabled={busy}
                            onClick={() => openInteractiveStep(step.id, true)}
                          >
                            Open setup portal
                          </Button>
                        )}
                      {step.id === "installHermes" && reviewedInstallerCommand(activeRecipe) && (
                        <Button
                          variant="danger"
                          disabled={busy}
                          onClick={() => setInstallScriptApprovalOpen(true)}
                        >
                          Run reviewed installer
                        </Button>
                      )}
                    </>
                  ) : (
                    <Button
                      variant="ghost"
                      disabled={busy || step.state === "done" || step.state === "blocked"}
                      onClick={() => void beginRun(step.id, true)}
                    >
                      Run step
                    </Button>
                  )}
                  {step.id === "configureHermes" && (
                    <Button variant="ghost" onClick={() => go("terminal")}>
                      Open terminal
                    </Button>
                  )}
                </span>
              </li>
            ))}
          </ol>
        )}

        {error && <ErrorPanel error={error} />}
        {output.length > 0 && (
          <pre
            aria-label="Provisioning output"
            aria-live="polite"
            className="mt-4 max-h-56 overflow-auto rounded border border-border bg-bg p-3 font-mono text-[12px]"
          >
            {output.join("\n")}
          </pre>
        )}
      </Card>

      <ConfirmDialog
        open={consentPrompt !== null}
        title={consentPrompt?.consent.title ?? "Approve provisioning step?"}
        confirmLabel="Approve and continue"
        onConfirm={() => {
          if (!consentPrompt) return;
          const prompt = consentPrompt;
          setConsentPrompt(null);
          void beginRun(prompt.step, prompt.onlyStep, [prompt.step]);
        }}
        onCancel={() => setConsentPrompt(null)}
      >
        {consentPrompt && (
          <>
            <p className="mb-2">Review the commands and changes for this phone:</p>
            <ul className="list-disc space-y-1 pl-5 font-mono text-[12px]">
              {consentPrompt.consent.commands.map((command) => (
                <li key={command}>{command}</li>
              ))}
            </ul>
            {consentPrompt.consent.destructive && (
              <p className="mt-3 text-danger">This step may delete existing phone data.</p>
            )}
          </>
        )}
      </ConfirmDialog>

      <ConfirmDialog
        open={installScriptApprovalOpen}
        title={`Run the reviewed Hermes installer on ${serial}?`}
        confirmLabel="Run installer"
        danger
        onConfirm={runReviewedInstaller}
        onCancel={() => setInstallScriptApprovalOpen(false)}
      >
        <p className="mb-3 text-danger">
          This executes the script downloaded by View installer script. Review its contents and
          checksum in the terminal before continuing.
        </p>
        <pre className="overflow-auto rounded border border-border bg-bg p-2 font-mono text-[12px]">
          {reviewedInstallerCommand(activeRecipe)}
        </pre>
      </ConfirmDialog>

      <ConfirmDialog
        open={resetPromptOpen}
        title="Reset provisioning progress?"
        confirmLabel="Reset progress"
        danger
        onConfirm={() => void resetProgress()}
        onCancel={() => setResetPromptOpen(false)}
      >
        <p>
          This clears the saved checklist for this phone and recipe. It does not undo changes
          already made on the phone.
        </p>
      </ConfirmDialog>

      <Dialog
        open={termuxUninstallOpen}
        title={`Uninstall Termux from ${serial}?`}
        onClose={() => {
          if (uninstallingTermux) return;
          setTermuxUninstallOpen(false);
          setTermuxUninstallConfirmation("");
        }}
        footer={
          <>
            <Button
              disabled={uninstallingTermux}
              onClick={() => {
                setTermuxUninstallOpen(false);
                setTermuxUninstallConfirmation("");
              }}
            >
              Cancel
            </Button>
            <Button
              variant="danger"
              loading={uninstallingTermux}
              disabled={termuxUninstallConfirmation !== TERMUX_UNINSTALL_CONFIRMATION}
              onClick={() => void uninstallIncompatibleTermux()}
            >
              Uninstall Termux
            </Button>
          </>
        }
      >
        <p className="mb-3 text-danger">
          This permanently removes Termux and deletes all of its data from {serial}. Provisioning
          will continue only after the phone passes preflight again.
        </p>
        <label className="flex flex-col gap-1">
          <span className="text-muted">Type UNINSTALL TERMUX to confirm</span>
          <input
            aria-label="Type UNINSTALL TERMUX to confirm"
            autoComplete="off"
            className="rounded-md border border-border bg-bg px-2 py-1.5"
            value={termuxUninstallConfirmation}
            onChange={(event) => setTermuxUninstallConfirmation(event.target.value)}
          />
        </label>
      </Dialog>

      <Dialog
        open={recipeEditorOpen}
        title="Provisioning recipe"
        onClose={() => setRecipeEditorOpen(false)}
        footer={
          <>
            <Button onClick={() => setRecipeEditorOpen(false)}>Cancel</Button>
            <Button
              variant="primary"
              loading={recipeEditorBusy}
              onClick={() => void saveRecipeEditor()}
            >
              Save recipe
            </Button>
          </>
        }
      >
        {(() => {
          const draft = parseRecipeDraft(recipeEditorSource);
          const setField = (path: string[], value: unknown) => {
            const nextSource = setRecipeDraftValue(recipeEditorSource, path, value);
            if (nextSource) setRecipeEditorSource(nextSource);
          };
          const stringField = (path: string[], label: string, value: unknown) => (
            <label className="flex flex-col gap-1">
              <span className="text-muted">{label}</span>
              <input
                aria-label={label}
                className="rounded-md border border-border bg-bg px-2 py-1.5"
                value={typeof value === "string" ? value : ""}
                onChange={(event) => setField(path, event.target.value)}
              />
            </label>
          );
          const listValue = (value: unknown): string =>
            Array.isArray(value)
              ? value.filter((item): item is string => typeof item === "string").join(", ")
              : "";
          const listField = (path: string[], label: string, value: unknown) => (
            <label className="flex flex-col gap-1">
              <span className="text-muted">{label}</span>
              <input
                aria-label={label}
                className="rounded-md border border-border bg-bg px-2 py-1.5"
                value={listValue(value)}
                onChange={(event) =>
                  setField(
                    path,
                    event.target.value.split(",").map((item) => item.trim()),
                  )
                }
                onBlur={() => {
                  const parsed = parseRecipeDraft(recipeEditorSource);
                  const existing = path.reduce<unknown>((valueAtPath, key) => {
                    if (typeof valueAtPath !== "object" || valueAtPath === null) return undefined;
                    return (valueAtPath as Record<string, unknown>)[key];
                  }, parsed);
                  setField(path, Array.isArray(existing) ? existing.filter(Boolean) : []);
                }}
              />
            </label>
          );
          const install =
            typeof draft?.hermesInstall === "object" && draft.hermesInstall !== null
              ? (draft.hermesInstall as Record<string, unknown>)
              : {};
          const runtime =
            typeof draft?.hermesRuntime === "object" && draft.hermesRuntime !== null
              ? (draft.hermesRuntime as Record<string, unknown>)
              : {};
          return (
            <div className="flex max-h-[70vh] flex-col gap-3 overflow-auto">
              {draft ? (
                <div className="grid grid-cols-2 gap-3">
                  {stringField(["id"], "Recipe ID", draft.id)}
                  {stringField(["name"], "Recipe name", draft.name)}
                  {stringField(["distro"], "proot distro (blank for native)", draft.distro)}
                  {stringField(
                    ["minimumTermuxVersion"],
                    "Minimum Termux version",
                    draft.minimumTermuxVersion,
                  )}
                  <label className="flex flex-col gap-1">
                    <span className="text-muted">Minimum free storage (GiB)</span>
                    <input
                      aria-label="Minimum free storage (GiB)"
                      className="rounded-md border border-border bg-bg px-2 py-1.5"
                      min={1}
                      step={1}
                      type="number"
                      value={Number(draft.minimumFreeGib ?? 2)}
                      onChange={(event) => setField(["minimumFreeGib"], Number(event.target.value))}
                    />
                  </label>
                  <label className="flex flex-col gap-1">
                    <span className="text-muted">Termux source</span>
                    <select
                      aria-label="Recipe Termux source"
                      className="rounded-md border border-border bg-bg px-2 py-1.5"
                      value={String(draft.termuxSource ?? "fdroid")}
                      onChange={(event) => setField(["termuxSource"], event.target.value)}
                    >
                      <option value="fdroid">F-Droid</option>
                      <option value="github">GitHub</option>
                    </select>
                  </label>
                  {stringField(
                    ["hermesInstall", "scriptUrl"],
                    "Hermes installer URL",
                    install.scriptUrl,
                  )}
                  {stringField(
                    ["hermesInstall", "nativeAptPackage"],
                    "Native APT package (optional)",
                    install.nativeAptPackage,
                  )}
                  {stringField(
                    ["hermesInstall", "aptKeyFingerprint"],
                    "APT signing-key fingerprint",
                    install.aptKeyFingerprint,
                  )}
                  {listField(
                    ["hermesConfigure", "steps"],
                    "Hermes setup commands",
                    (draft.hermesConfigure as Record<string, unknown> | undefined)?.steps,
                  )}
                  {stringField(
                    ["hermesRuntime", "gatewayCommand"],
                    "Gateway command",
                    runtime.gatewayCommand,
                  )}
                  {stringField(
                    ["hermesRuntime", "processMatch"],
                    "Process match",
                    runtime.processMatch,
                  )}
                  {listField(
                    ["hermesRuntime", "statusCommands"],
                    "Status commands",
                    runtime.statusCommands,
                  )}
                  {stringField(
                    ["hermesRuntime", "versionCommand"],
                    "Version command",
                    runtime.versionCommand,
                  )}
                  {stringField(
                    ["hermesRuntime", "doctorCommand"],
                    "Doctor command",
                    runtime.doctorCommand,
                  )}
                  {stringField(
                    ["hermesRuntime", "updateCommand"],
                    "Update command",
                    runtime.updateCommand,
                  )}
                  {listField(["hermesRuntime", "logFiles"], "Hermes log files", runtime.logFiles)}
                  {stringField(
                    ["hermesRuntime", "stateFile"],
                    "Hermes state file",
                    runtime.stateFile,
                  )}
                  {listField(
                    ["hermesRuntime", "pathPrepend"],
                    "PATH directories",
                    runtime.pathPrepend,
                  )}
                  {listField(["termuxPackages"], "Termux packages", draft.termuxPackages)}
                  {listField(["distroPackages"], "Distro packages", draft.distroPackages)}
                  <label className="flex items-center gap-2">
                    <input
                      aria-label="Enable Termux boot autostart"
                      type="checkbox"
                      checked={draft.autostart === true}
                      onChange={(event) => setField(["autostart"], event.target.checked)}
                    />
                    Enable Termux:Boot
                  </label>
                  <label className="flex items-center gap-2">
                    <input
                      aria-label="Mark recipe experimental"
                      type="checkbox"
                      checked={draft.experimental === true}
                      onChange={(event) => setField(["experimental"], event.target.checked)}
                    />
                    Experimental
                  </label>
                </div>
              ) : (
                <p role="alert" className="text-danger">
                  Recipe TOML is invalid; edit the source below to continue.
                </p>
              )}
              <label className="flex flex-col gap-1">
                <span className="text-muted">Raw TOML</span>
                <textarea
                  aria-label="Raw recipe TOML"
                  className="min-h-52 resize-y rounded-md border border-border bg-bg p-2 font-mono text-[12px]"
                  value={recipeEditorSource}
                  onChange={(event) => setRecipeEditorSource(event.target.value)}
                  spellCheck={false}
                />
              </label>
              <p className="text-muted">
                Bundled recipes are read-only. Duplicate one to save a customized copy.
              </p>
            </div>
          );
        })()}
      </Dialog>
    </>
  );
}
