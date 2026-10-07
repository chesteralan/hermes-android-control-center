import { useEffect, useState, type ReactNode } from "react";
import { Button } from "../../components/Button";
import { Card } from "../../components/Card";
import { ConfirmDialog } from "../../components/ConfirmDialog";
import { ErrorPanel } from "../../components/ErrorPanel";
import { ipc } from "../../lib/ipc";
import { useDevices } from "../../stores/devices";
import { isDirty, useSettings } from "../../stores/settings";
import { useToasts } from "../../stores/toast";
import type {
  AppConfig,
  ControlApiInstallPreview,
  HermesConfig,
  HermesInstallReport,
  HermesTransportKind,
  LogLevelSetting,
  StartMode,
} from "../../types";

const input = "w-full rounded-md border border-border bg-bg px-2 py-1.5 font-mono";

function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[200px_1fr] items-start gap-4 py-2">
      <span className="pt-1.5">
        {label}
        {hint && <span className="block text-[12px] text-muted">{hint}</span>}
      </span>
      <div>{children}</div>
    </div>
  );
}

const HERMES_COMMAND_FIELDS: Array<[keyof HermesConfig, string, string]> = [
  ["gatewayCommand", "Gateway command", "Command run by the supervisor."],
  ["processMatch", "Process match", "Substring matched against the Hermes process command line."],
  [
    "gatewayMatch",
    "Gateway match",
    "Additional command-line text identifying the gateway process.",
  ],
  ["hermesHome", "Hermes home", "Path inside the selected environment."],
  ["startCommand", "Start override", "Optional; leave blank to use the selected start mode."],
  ["stopCommand", "Stop override", "Optional; leave blank to use the managed stop action."],
  ["restartCommand", "Restart override", "Optional; leave blank to use managed restart."],
  ["statusCommand", "Status command", "Optional command used for extra status information."],
  ["logCommand", "Log command", "Optional; e.g. tail -n 200 -F ~/.hermes/logs/gateway.log"],
  ["versionCommand", "Version command", "Run inside the selected Hermes environment."],
  ["doctorCommand", "Doctor command", "Run Hermes diagnostics."],
  ["updateCommand", "Update command", "Runs Hermes update, then restarts the gateway."],
];

function shellQuote(value: string): string {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}

export function previewHermesCommand(config: HermesConfig): string {
  const path = config.pathPrepend.length
    ? `export PATH=${config.pathPrepend.map(shellQuote).join(":")}:$PATH; `
    : "";
  const inner = `${path}${config.gatewayCommand}`;
  if (config.environment.type === "termux") return inner;
  return `proot-distro login ${shellQuote(config.environment.distro)} -- bash -c ${shellQuote(inner)}`;
}
export function SettingsView() {
  const s = useSettings();
  const adb = useDevices((st) => st.adb);
  const detect = useDevices((st) => st.detectAdb);
  const activeSerial = useDevices((st) =>
    st.activeKey ? (st.devices[st.activeKey]?.serial ?? null) : null,
  );
  const toast = useToasts((t) => t.push);
  const { load } = s;
  const [detection, setDetection] = useState<HermesInstallReport | null>(null);
  const [detecting, setDetecting] = useState(false);
  const [controlApiPreview, setControlApiPreview] = useState<ControlApiInstallPreview | null>(null);
  const [previewingControlApi, setPreviewingControlApi] = useState(false);
  const [installingControlApi, setInstallingControlApi] = useState(false);
  const [testingControlApi, setTestingControlApi] = useState(false);
  const [diagnosticsDialogOpen, setDiagnosticsDialogOpen] = useState(false);
  const [redactDiagnosticsIps, setRedactDiagnosticsIps] = useState(true);
  const [exportingDiagnostics, setExportingDiagnostics] = useState(false);
  const [secretStorage, setSecretStorage] = useState<
    "native" | "encryptedLocked" | "encryptedUnlocked" | null
  >(null);
  const [vaultPassphrase, setVaultPassphrase] = useState("");
  const [vaultOptedIn, setVaultOptedIn] = useState(false);
  const [vaultBusy, setVaultBusy] = useState(false);
  const [vaultError, setVaultError] = useState("");

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    let active = true;
    void ipc
      .getSecretStorageState()
      .then((status) => {
        if (active) setSecretStorage(status);
      })
      .catch(() => {
        if (active) setVaultError("Could not inspect secret storage.");
      });
    return () => {
      active = false;
    };
  }, []);

  const d = s.draft;
  if (!d) return s.error ? <ErrorPanel error={s.error} onRetry={load} /> : null;
  const edit = (fn: (c: AppConfig) => AppConfig) => s.edit(fn);
  const dirty = isDirty(s);

  async function detectHermes() {
    const device = useDevices.getState().activeKey
      ? useDevices.getState().devices[useDevices.getState().activeKey!]
      : null;
    if (!device) return;
    setDetecting(true);
    try {
      setDetection(await ipc.detectHermes(device.serial));
    } catch (e) {
      useToasts
        .getState()
        .push((e as { message?: string }).message ?? "Hermes detection failed", "error");
    } finally {
      setDetecting(false);
    }
  }

  async function previewControlApi() {
    if (!activeSerial) return;
    setPreviewingControlApi(true);
    try {
      setControlApiPreview(await ipc.previewControlApiInstall(activeSerial));
    } catch (e) {
      toast((e as { message?: string }).message ?? "Control API preview failed", "error");
    } finally {
      setPreviewingControlApi(false);
    }
  }

  async function installControlApi() {
    if (!activeSerial || installingControlApi) return;
    setInstallingControlApi(true);
    try {
      await ipc.installControlApi(activeSerial);
      setControlApiPreview(null);
      toast("Control API installed and enabled", "success");
      try {
        const version = await ipc.testControlApi(activeSerial);
        toast(`Control API ${version} is reachable`, "success");
      } catch (e) {
        toast((e as { message?: string }).message ?? "Control API test failed", "error");
      }
    } catch (e) {
      toast((e as { message?: string }).message ?? "Control API installation failed", "error");
    } finally {
      setInstallingControlApi(false);
    }
  }

  async function testControlApi() {
    if (!activeSerial) return;
    setTestingControlApi(true);
    try {
      const version = await ipc.testControlApi(activeSerial);
      toast(`Control API ${version} is reachable`, "success");
    } catch (e) {
      toast((e as { message?: string }).message ?? "Control API test failed", "error");
    } finally {
      setTestingControlApi(false);
    }
  }

  async function exportDiagnostics() {
    setExportingDiagnostics(true);
    try {
      const path = await ipc.exportDiagnostics(redactDiagnosticsIps);
      if (path) toast("Diagnostics archive exported", "success");
      setDiagnosticsDialogOpen(false);
    } catch (e) {
      toast((e as { message?: string }).message ?? "Diagnostics export failed", "error");
    } finally {
      setExportingDiagnostics(false);
    }
  }

  async function unlockVault(): Promise<void> {
    setVaultBusy(true);
    setVaultError("");
    try {
      await ipc.unlockSecretStorage(vaultPassphrase, vaultOptedIn);
      setSecretStorage("encryptedUnlocked");
      setVaultOptedIn(false);
    } catch (cause) {
      setVaultError(
        (cause as { message?: string }).message ?? "Could not unlock encrypted storage.",
      );
    } finally {
      setVaultPassphrase("");
      setVaultBusy(false);
    }
  }

  async function lockVault(): Promise<void> {
    try {
      await ipc.lockSecretStorage();
      setSecretStorage("encryptedLocked");
      setVaultPassphrase("");
    } catch {
      setVaultError("Could not lock encrypted storage.");
    }
  }

  return (
    <div className="flex flex-col gap-4 pb-16">
      <Card title="ADB">
        <Row label="ADB path" hint="Leave empty to auto-detect.">
          <div className="flex gap-2">
            <input
              aria-label="ADB path"
              className={input}
              value={d.adbPath ?? ""}
              placeholder={adb.info?.path ?? "/opt/homebrew/bin/adb"}
              onChange={(e) => edit((c) => ({ ...c, adbPath: e.target.value.trim() || null }))}
            />
            <Button onClick={() => void detect()}>Detect</Button>
          </div>
          <div className="mt-2">
            {adb.info && (
              <span className="text-muted">
                Found adb {adb.info.version} at <span className="font-mono">{adb.info.path}</span>
              </span>
            )}
            {adb.error && <ErrorPanel error={adb.error} />}
          </div>
        </Row>
      </Card>

      <Card title="Connection">
        <Row label="Auto-reconnect">
          <input
            type="checkbox"
            aria-label="Auto-reconnect"
            checked={d.reconnect.enabled}
            onChange={(e) =>
              edit((c) => ({ ...c, reconnect: { ...c.reconnect, enabled: e.target.checked } }))
            }
          />
        </Row>
        <Row label="Remembered phones" hint="Auto-connected on launch.">
          {d.knownAddresses.length === 0 && <span className="text-muted">None yet.</span>}
          {d.knownAddresses.map((k, i) => (
            <div key={k.address} className="flex items-center gap-3 py-0.5">
              <span className="font-mono">{k.address}</span>
              <label className="flex items-center gap-1 text-muted">
                <input
                  type="checkbox"
                  checked={k.autoConnect}
                  onChange={(e) =>
                    edit((c) => ({
                      ...c,
                      knownAddresses: c.knownAddresses.map((x, j) =>
                        j === i ? { ...x, autoConnect: e.target.checked } : x,
                      ),
                    }))
                  }
                />
                auto-connect
              </label>
              <Button
                variant="ghost"
                onClick={() =>
                  edit((c) => ({
                    ...c,
                    knownAddresses: c.knownAddresses.filter((_, j) => j !== i),
                  }))
                }
              >
                Forget
              </Button>
            </div>
          ))}
        </Row>
      </Card>

      <Card
        title="Hermes"
        actions={
          <Button onClick={detectHermes} loading={detecting}>
            Detect Hermes
          </Button>
        }
      >
        <Row
          label="Environment"
          hint="Where Hermes is installed; separate from its SSH/API transport."
        >
          <div className="flex gap-2">
            <select
              aria-label="Hermes environment"
              className={`${input} w-48`}
              value={d.hermes.environment.type}
              onChange={(e) =>
                edit((c) => ({
                  ...c,
                  hermes: {
                    ...c.hermes,
                    environment:
                      e.target.value === "termux"
                        ? { type: "termux" }
                        : {
                            type: "prootDistro",
                            distro:
                              c.hermes.environment.type === "prootDistro"
                                ? c.hermes.environment.distro
                                : "debian",
                          },
                  },
                }))
              }
            >
              <option value="prootDistro">proot-distro</option>
              <option value="termux">Termux</option>
            </select>
            {d.hermes.environment.type === "prootDistro" && (
              <input
                aria-label="proot distro"
                className={`${input} w-40`}
                value={d.hermes.environment.distro}
                list="hermes-distros"
                onChange={(e) =>
                  edit((c) => ({
                    ...c,
                    hermes: {
                      ...c.hermes,
                      environment: { type: "prootDistro", distro: e.target.value },
                    },
                  }))
                }
              />
            )}
            <datalist id="hermes-distros">
              {(detection?.candidates ?? []).flatMap((c) =>
                c.environment.type === "prootDistro"
                  ? [<option key={c.environment.distro}>{c.environment.distro}</option>]
                  : [],
              )}
            </datalist>
          </div>
        </Row>
        <Row
          label="Start mode"
          hint="Supervised restarts the gateway after crashes and Hermes restarts."
        >
          <select
            aria-label="Hermes start mode"
            className={`${input} w-48`}
            value={d.hermes.startMode}
            onChange={(e) =>
              edit((c) => ({
                ...c,
                hermes: { ...c.hermes, startMode: e.target.value as StartMode },
              }))
            }
          >
            <option value="supervised">Supervised</option>
            <option value="detached">Detached</option>
          </select>
        </Row>
        <Row label="Hermes transport" hint="Control API traffic is tunneled over ADB.">
          <select
            aria-label="Hermes transport"
            className={`${input} w-48`}
            value={d.hermes.transport}
            onChange={(e) =>
              edit((c) => ({
                ...c,
                hermes: { ...c.hermes, transport: e.target.value as HermesTransportKind },
              }))
            }
          >
            <option value="termuxSsh">Termux SSH</option>
            <option value="controlApi">Control API</option>
          </select>
        </Row>
        {d.hermes.transport === "controlApi" && (
          <>
            <Row label="SSH fallback" hint="Use SSH only when the Control API is unavailable.">
              <input
                type="checkbox"
                aria-label="Fallback to Termux SSH"
                checked={d.hermes.fallbackToSsh}
                onChange={(e) =>
                  edit((c) => ({
                    ...c,
                    hermes: { ...c.hermes, fallbackToSsh: e.target.checked },
                  }))
                }
              />
            </Row>
            <Row
              label="Control API service"
              hint="Install or update, then test the device connection."
            >
              <div className="flex gap-2">
                <Button
                  disabled={!activeSerial}
                  loading={previewingControlApi}
                  onClick={() => void previewControlApi()}
                >
                  Review install
                </Button>
                <Button
                  disabled={!activeSerial}
                  loading={testingControlApi}
                  onClick={() => void testControlApi()}
                >
                  Test
                </Button>
              </div>
            </Row>
          </>
        )}
        {HERMES_COMMAND_FIELDS.map(([key, label, hint]) => (
          <Row key={key} label={label} hint={hint}>
            <input
              aria-label={label}
              className={input}
              value={d.hermes[key] as string}
              onChange={(e) =>
                edit((c) => ({ ...c, hermes: { ...c.hermes, [key]: e.target.value } }))
              }
            />
          </Row>
        ))}
        <Row
          label="PATH directories"
          hint="Comma-separated; these precede PATH inside the environment."
        >
          <input
            aria-label="PATH directories"
            className={input}
            value={d.hermes.pathPrepend.join(", ")}
            onChange={(e) =>
              edit((c) => ({
                ...c,
                hermes: {
                  ...c.hermes,
                  pathPrepend: e.target.value
                    .split(",")
                    .map((x) => x.trim())
                    .filter(Boolean),
                },
              }))
            }
          />
        </Row>
        <Row
          label="Hermes log files"
          hint="Comma-separated paths inside the configured Hermes environment; gateway first, tool calls second."
        >
          <input
            aria-label="Hermes log files"
            className={input}
            value={d.hermes.logFiles.join(", ")}
            onChange={(e) =>
              edit((c) => ({
                ...c,
                hermes: {
                  ...c.hermes,
                  logFiles: e.target.value
                    .split(",")
                    .map((path) => path.trim())
                    .filter(Boolean),
                },
              }))
            }
          />
        </Row>
        <Row label="Command preview" hint="Gateway command wrapped for the selected environment.">
          <code className="block overflow-x-auto whitespace-pre rounded bg-bg p-2 font-mono text-[12px]">
            {previewHermesCommand(d.hermes)}
          </code>
        </Row>
        {detection && (
          <div className="mt-3 border-t border-border pt-3">
            <h3 className="mb-2 font-medium">Detected installations</h3>
            {detection.candidates.length === 0 ? (
              <p className="text-muted">
                Hermes not found. Searched: {detection.searched.join(", ") || "none"}.
              </p>
            ) : (
              <ul className="flex flex-col gap-2">
                {detection.candidates.map((candidate) => {
                  const envLabel =
                    candidate.environment.type === "termux"
                      ? "Termux"
                      : candidate.environment.distro;
                  return (
                    <li
                      key={`${envLabel}:${candidate.binaryPath}`}
                      className="flex items-center justify-between gap-3"
                    >
                      <span className="font-mono text-[12px]">
                        {envLabel}: {candidate.binaryPath}
                        {candidate.pythonVersion ? ` · ${candidate.pythonVersion}` : ""}
                      </span>
                      <Button
                        onClick={() =>
                          edit((c) => ({
                            ...c,
                            hermes: {
                              ...c.hermes,
                              environment: candidate.environment,
                              pathPrepend: [candidate.binDir],
                              hermesHome:
                                candidate.environment.type === "termux"
                                  ? "~/.hermes"
                                  : "/root/.hermes",
                              processMatch:
                                candidate.environment.type === "termux"
                                  ? "hermes"
                                  : c.hermes.processMatch,
                            },
                          }))
                        }
                      >
                        Use this
                      </Button>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>
        )}
      </Card>

      <Card title="Logs & API">
        <Row label="Logcat filter" hint="Arguments for adb logcat.">
          <input
            aria-label="Logcat filter"
            className={input}
            value={d.logs.logcatFilter}
            onChange={(e) =>
              edit((c) => ({ ...c, logs: { ...c.logs, logcatFilter: e.target.value } }))
            }
          />
        </Row>
        <Row label="Auto-start log streaming">
          <input
            type="checkbox"
            aria-label="Auto-start log streaming"
            checked={d.logs.autoStart}
            onChange={(e) =>
              edit((c) => ({ ...c, logs: { ...c.logs, autoStart: e.target.checked } }))
            }
          />
        </Row>
        <Row label="API port" hint="Hermes Control API on the phone (M8).">
          <input
            aria-label="API port"
            className={`${input} w-28`}
            inputMode="numeric"
            value={String(d.apiPort)}
            onChange={(e) => edit((c) => ({ ...c, apiPort: Number(e.target.value) || 0 }))}
          />
        </Row>
        <Row label="App log level">
          <select
            aria-label="App log level"
            className={`${input} w-32`}
            value={d.logLevel}
            onChange={(e) => edit((c) => ({ ...c, logLevel: e.target.value as LogLevelSetting }))}
          >
            {(["error", "warn", "info", "debug"] as LogLevelSetting[]).map((l) => (
              <option key={l}>{l}</option>
            ))}
          </select>
        </Row>
      </Card>

      <Card title="Secret storage">
        <Row label="Token storage">
          <span role="status">
            {secretStorage === "native"
              ? "System credential store"
              : secretStorage === "encryptedUnlocked"
                ? "Encrypted file - unlocked"
                : secretStorage === "encryptedLocked"
                  ? "Encrypted file - locked"
                  : "Checking storage..."}
          </span>
        </Row>
        {secretStorage === "encryptedUnlocked" ? (
          <Row label="Encrypted vault">
            <Button onClick={() => void lockVault()}>Lock vault</Button>
          </Row>
        ) : (
          <>
            <Row label="Encrypted file storage">
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  aria-label="Use passphrase-encrypted file storage"
                  checked={vaultOptedIn}
                  disabled={vaultBusy}
                  onChange={(event) => setVaultOptedIn(event.target.checked)}
                />
                Use passphrase-encrypted file storage
              </label>
            </Row>
            <Row label="Vault passphrase">
              <input
                type="password"
                aria-label="Vault passphrase"
                autoComplete="current-password"
                className={input}
                value={vaultPassphrase}
                disabled={vaultBusy}
                onChange={(event) => setVaultPassphrase(event.target.value)}
              />
            </Row>
            <p className="mb-3 text-warning">
              Existing system-store tokens are not migrated. Losing the passphrase makes encrypted
              tokens unrecoverable.
            </p>
            <Button
              disabled={
                !vaultOptedIn || vaultPassphrase.length < 12 || vaultBusy || secretStorage === null
              }
              onClick={() => void unlockVault()}
            >
              {vaultBusy
                ? "Unlocking..."
                : secretStorage === "encryptedLocked"
                  ? "Unlock vault"
                  : "Enable encrypted storage"}
            </Button>
          </>
        )}
        {vaultError && (
          <p role="alert" className="mt-2 text-danger">
            {vaultError}
          </p>
        )}
      </Card>

      <Card title="Diagnostics">
        <Row label="Export diagnostics" hint="Create a ZIP for troubleshooting reports.">
          <div className="flex items-center gap-3">
            <label className="flex items-center gap-2 text-muted">
              <input
                type="checkbox"
                aria-label="Redact IP addresses in diagnostics"
                checked={redactDiagnosticsIps}
                onChange={(event) => setRedactDiagnosticsIps(event.target.checked)}
              />
              Redact IP addresses
            </label>
            <Button onClick={() => setDiagnosticsDialogOpen(true)}>Export diagnostics</Button>
          </div>
        </Row>
      </Card>

      {s.error && <ErrorPanel error={s.error} />}

      <ConfirmDialog
        open={diagnosticsDialogOpen}
        title="Export diagnostics?"
        confirmLabel={exportingDiagnostics ? "Exporting..." : "Export ZIP"}
        onConfirm={() => void exportDiagnostics()}
        onCancel={() => setDiagnosticsDialogOpen(false)}
      >
        <p>
          The ZIP includes redacted settings, recent app logs, device details, and up to 500 Hermes
          gateway log lines per connected phone. Tokens are always redacted
          {redactDiagnosticsIps ? " and IP addresses are redacted" : ""}.
        </p>
        <p className="mt-3 text-warning">
          Logs may contain conversation or other private content. Review the archive before sharing.
        </p>
      </ConfirmDialog>

      {controlApiPreview && (
        <ConfirmDialog
          open
          title="Review Control API update"
          confirmLabel={controlApiPreview.installedVersion ? "Upgrade" : "Install"}
          onConfirm={() => void installControlApi()}
          onCancel={() => setControlApiPreview(null)}
        >
          <p>
            Version: {controlApiPreview.installedVersion ?? "Not installed"} to{" "}
            {controlApiPreview.targetVersion}
          </p>
          <p className="mt-3">Files to update:</p>
          <ul className="mt-1 max-h-48 overflow-y-auto font-mono text-[12px]">
            {controlApiPreview.filesToUpdate.map((file) => (
              <li key={file}>{file}</li>
            ))}
          </ul>
          {installingControlApi && <p className="mt-3 text-muted">Installing service...</p>}
        </ConfirmDialog>
      )}

      <div className="fixed bottom-0 right-0 left-[200px] flex justify-end gap-2 border-t border-border bg-surface px-6 py-3">
        <Button variant="ghost" disabled={!dirty} onClick={s.revert}>
          Revert
        </Button>
        <Button
          variant="primary"
          disabled={!dirty}
          loading={s.saving}
          onClick={async () => {
            if (await s.save()) toast("Settings saved", "success");
          }}
        >
          Save
        </Button>
      </div>
    </div>
  );
}
