import { useEffect, type ReactNode } from "react";
import { Button } from "../../components/Button";
import { Card } from "../../components/Card";
import { ErrorPanel } from "../../components/ErrorPanel";
import { useDevices } from "../../stores/devices";
import { isDirty, useSettings } from "../../stores/settings";
import { useToasts } from "../../stores/toast";
import type { AppConfig, HermesConfig, LogLevelSetting } from "../../types";

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

const HERMES_FIELDS: Array<[keyof HermesConfig, string, string]> = [
  ["startCommand", "Start command", "Run in Termux (M4+)."],
  ["stopCommand", "Stop command", ""],
  ["restartCommand", "Restart command", "Empty = stop, then start."],
  ["statusCommand", "Status command", ""],
  ["logCommand", "Log command", "e.g. tail -n 200 -F <log file>"],
  ["processMatch", "Process match", "Pattern used to find the Hermes process."],
];

export function SettingsView() {
  const s = useSettings();
  const adb = useDevices((st) => st.adb);
  const detect = useDevices((st) => st.detectAdb);
  const toast = useToasts((t) => t.push);
  const { load } = s;

  useEffect(() => {
    void load();
  }, [load]);

  const d = s.draft;
  if (!d) return s.error ? <ErrorPanel error={s.error} onRetry={load} /> : null;
  const edit = (fn: (c: AppConfig) => AppConfig) => s.edit(fn);
  const dirty = isDirty(s);

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

      <Card title="Hermes">
        {HERMES_FIELDS.map(([key, label, hint]) => (
          <Row key={key} label={label} hint={hint}>
            <input
              aria-label={label}
              className={input}
              value={d.hermes[key]}
              onChange={(e) =>
                edit((c) => ({ ...c, hermes: { ...c.hermes, [key]: e.target.value } }))
              }
            />
          </Row>
        ))}
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
