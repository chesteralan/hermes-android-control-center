import { useEffect, useState, type ReactNode } from "react";
import { Button } from "../../components/Button";
import { Card } from "../../components/Card";
import { ErrorPanel } from "../../components/ErrorPanel";
import { ipc } from "../../lib/ipc";
import { useDevices } from "../../stores/devices";
import { isDirty, useSettings } from "../../stores/settings";
import { useToasts } from "../../stores/toast";
import type {
  AppConfig,
  HermesConfig,
  HermesInstallReport,
  LogLevelSetting,
  StartMode,
} from "../../types";

const input = "w-full rounded-md border border-border bg-bg px-2 py-1.5 font-mono";

function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="grid grid-cols-[200px_1fr] items-start gap-4 py-2">
      <span className="pt-1.5">
        {label}
        {hint && <span className="block text-[12px] text-muted">{hint}</span>}
      </span>
      <span>{children}</span>
    </label>
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
  const toast = useToasts((t) => t.push);
  const { load } = s;
  const [detection, setDetection] = useState<HermesInstallReport | null>(null);
  const [detecting, setDetecting] = useState(false);

  useEffect(() => {
    void load();
  }, [load]);

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

      {s.error && <ErrorPanel error={s.error} />}

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
