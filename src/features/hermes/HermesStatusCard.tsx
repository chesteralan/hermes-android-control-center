import { useEffect, useState } from "react";
import { Button } from "../../components/Button";
import { Card } from "../../components/Card";
import { ConfirmDialog } from "../../components/ConfirmDialog";
import { ErrorPanel } from "../../components/ErrorPanel";
import { KeyValue } from "../../components/KeyValue";
import { StatusDot, type Tone } from "../../components/StatusDot";
import { formatSeconds } from "../../lib/format";
import { deviceKey, useActiveDevice } from "../../stores/devices";
import { EMPTY_HERMES, useHermes } from "../../stores/hermes";
import { useRoute } from "../../stores/route";
import type { ComponentStatus, HermesAction, HermesCandidate, HermesTool } from "../../types";
import { NoDeviceState } from "../device/NoDeviceState";
import { deviceTitle } from "../device/deviceStatus";

const POLL_MS = 10_000;

const STATUS_TONE: Record<ComponentStatus, Tone> = {
  running: "success",
  degraded: "warning",
  stopped: "muted",
  unknown: "muted",
};

function envName(candidate: HermesCandidate): string {
  return candidate.environment.type === "termux"
    ? "Termux"
    : `proot-distro · ${candidate.environment.distro}`;
}

function formatUptime(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return hours ? `${hours}h ${minutes}m` : formatSeconds(seconds * 1000);
}

export function HermesStatusCard({ compact = false }: { compact?: boolean }) {
  const device = useActiveDevice();
  const go = useRoute((s) => s.go);
  if (!device) return <NoDeviceState />;
  const key = deviceKey(device);
  return (
    <HermesCardForDevice
      device={device}
      deviceKey={key}
      compact={compact}
      onSettings={() => go("settings")}
    />
  );
}

function HermesCardForDevice({
  device,
  deviceKey: key,
  compact,
  onSettings,
}: {
  device: NonNullable<ReturnType<typeof useActiveDevice>>;
  deviceKey: string;
  compact: boolean;
  onSettings: () => void;
}) {
  const entry = useHermes((s) => s.byDevice[key] ?? EMPTY_HERMES);
  const error = entry.error ?? entry.statusError;
  const refresh = useHermes((s) => s.refresh);
  const detect = useHermes((s) => s.detect);
  const runAction = useHermes((s) => s.runAction);
  const runTool = useHermes((s) => s.runTool);
  const [confirm, setConfirm] = useState<HermesAction | null>(null);
  const [confirmUpdate, setConfirmUpdate] = useState(false);

  useEffect(() => {
    void refresh(key, device.serial);
    const timer = setInterval(() => void refresh(key, device.serial), POLL_MS);
    return () => clearInterval(timer);
  }, [device.serial, key, refresh]);

  const status = entry.status;
  const isRunning = status?.gateway === "running" || status?.gateway === "degraded";
  const agentState = status?.processes.length
    ? "Running"
    : status?.gateway === "unknown"
      ? "Unknown"
      : "Stopped";
  const runningAction = entry.action !== null;
  const runningTool = entry.tool !== null;
  const title = deviceTitle(device);

  const onAction = (action: HermesAction) => {
    setConfirm(null);
    void runAction(key, device.serial, action);
  };

  const onTool = (tool: HermesTool) => {
    setConfirmUpdate(false);
    void runTool(key, device.serial, tool);
  };

  return (
    <Card
      title={compact ? "Hermes Agent" : `Hermes · ${title}`}
      actions={
        <Button
          variant="ghost"
          loading={entry.loadingStatus}
          onClick={() => void refresh(key, device.serial)}
        >
          Refresh
        </Button>
      }
    >
      <div className="flex flex-col gap-4">
        {status ? (
          <>
            <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
              <div>
                <p className="mb-1 text-[11px] uppercase text-muted">Gateway</p>
                <StatusDot tone={STATUS_TONE[status.gateway]} label={status.gateway} />
              </div>
              <KeyValue
                rows={[
                  ["Agent", agentState],
                  ["PID", status.gatewayPid ?? "Unknown"],
                  [
                    "Uptime",
                    status.uptimeSecs == null ? "Unknown" : formatUptime(status.uptimeSecs),
                  ],
                  ["Python", status.pythonVersion ?? "Unknown"],
                  [
                    "Supervisor",
                    status.supervisor.running
                      ? `Running · ${status.supervisor.pid ?? "PID unknown"}`
                      : "Not running",
                  ],
                  [
                    "Source",
                    status.source === "adb"
                      ? "ADB (limited)"
                      : status.source === "api"
                        ? "Control API"
                        : "Termux SSH",
                  ],
                  ["Last checked", new Date(status.checkedAt * 1000).toLocaleTimeString()],
                ]}
              />
            </div>

            {status.processes.length > 0 && (
              <div>
                <p className="mb-1 text-[11px] uppercase text-muted">Hermes processes</p>
                <ul className="flex flex-col gap-1">
                  {status.processes.map((p) => (
                    <li
                      key={`${p.pid}:${p.kind}`}
                      className="flex flex-wrap gap-x-3 font-mono text-[12px]"
                    >
                      <span className="text-muted">{p.kind}</span>
                      <span>PID {p.pid}</span>
                      {p.uptimeSecs != null && (
                        <span className="text-muted">{formatUptime(p.uptimeSecs)}</span>
                      )}
                      <span className="text-muted">{p.cmdline}</span>
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {status.platforms.length > 0 && (
              <div>
                <p className="mb-1 text-[11px] uppercase text-muted">Messaging platforms</p>
                <ul className="grid grid-cols-2 gap-1">
                  {status.platforms.map((p) => (
                    <li
                      key={p.name}
                      className="flex items-center justify-between gap-3 text-[12px]"
                    >
                      <span>{p.name}</span>
                      <span className={p.state === "connected" ? "text-success" : "text-muted"}>
                        {p.state}
                        {p.error ? ` · ${p.error}` : ""}
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {status.warnings.length > 0 && (
              <ul className="flex flex-col gap-1 rounded-md border border-warning/30 bg-warning/5 p-2 text-warning">
                {status.warnings.map((warning) => (
                  <li key={warning}>{warning}</li>
                ))}
              </ul>
            )}
            {status.rawStatusOutput && (
              <details className="rounded-md border border-border bg-bg p-2 text-[12px]">
                <summary className="cursor-pointer text-muted">Status command output</summary>
                <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap font-mono">
                  {status.rawStatusOutput}
                </pre>
              </details>
            )}
          </>
        ) : (
          <StatusDot
            tone="muted"
            label={entry.loadingStatus ? "Checking Hermes…" : "Status unknown"}
            pulse={entry.loadingStatus}
          />
        )}

        {!compact && (
          <div className="border-t border-border pt-3">
            <div className="mb-2 flex items-center justify-between gap-3">
              <h3 className="text-[12px] font-semibold uppercase tracking-wider text-muted">
                Installation
              </h3>
              <Button
                variant="ghost"
                loading={entry.detecting}
                onClick={() => void detect(key, device.serial)}
              >
                Detect Hermes
              </Button>
            </div>
            {entry.detection ? (
              entry.detection.candidates.length ? (
                <ul className="flex flex-col gap-1">
                  {entry.detection.candidates.map((candidate) => (
                    <li
                      key={`${envName(candidate)}:${candidate.binaryPath}`}
                      className="flex flex-wrap items-baseline gap-x-3 text-[12px]"
                    >
                      <StatusDot tone="success" label={`Installed · ${envName(candidate)}`} />
                      <span className="font-mono text-muted">{candidate.binaryPath}</span>
                      {candidate.pythonVersion && (
                        <span className="text-muted">{candidate.pythonVersion}</span>
                      )}
                      {candidate.version && <span className="text-muted">{candidate.version}</span>}
                      {candidate.version ? ` · ${candidate.version}` : ""}
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="text-muted">
                  Hermes not found. Searched: {entry.detection.searched.join(", ") || "Unknown"}.
                </p>
              )
            ) : (
              <p className="text-muted">Installation status not checked.</p>
            )}
            {status?.pythonVersion && (
              <p className="mt-1 text-[12px] text-muted">
                Python reported by Hermes: {status.pythonVersion}
              </p>
            )}
          </div>
        )}

        {error && <ErrorPanel error={error} />}
        {entry.result && (
          <div className="rounded-md border border-border bg-bg p-2 text-[12px]">
            <div className="mb-1 flex justify-between">
              <span className="text-muted">
                {entry.result.confirmed
                  ? "Action confirmed"
                  : "Command completed; state not confirmed"}
              </span>
              <span className={entry.result.output.exitCode === 0 ? "text-success" : "text-danger"}>
                exit {entry.result.output.exitCode ?? "unknown"} · {entry.result.output.durationMs}{" "}
                ms
              </span>
            </div>
            {entry.result.output.stdout && (
              <pre className="whitespace-pre-wrap">{entry.result.output.stdout}</pre>
            )}
            {entry.result.output.stderr && (
              <pre className="whitespace-pre-wrap text-danger">{entry.result.output.stderr}</pre>
            )}
          </div>
        )}
        {entry.toolOutput && (
          <div className="rounded-md border border-border bg-bg p-2 text-[12px]">
            <div className="mb-1 flex items-center justify-between">
              <span className="text-muted">Hermes task complete</span>
              <span className={entry.toolOutput.exitCode === 0 ? "text-success" : "text-danger"}>
                exit {entry.toolOutput.exitCode ?? "unknown"} · {entry.toolOutput.durationMs} ms
              </span>
            </div>
            {entry.toolOutput.stdout && (
              <pre className="whitespace-pre-wrap">{entry.toolOutput.stdout}</pre>
            )}
            {entry.toolOutput.stderr && (
              <pre className="whitespace-pre-wrap text-danger">{entry.toolOutput.stderr}</pre>
            )}
          </div>
        )}

        <div className="flex flex-wrap items-center gap-2 border-t border-border pt-3">
          <Button
            variant="primary"
            disabled={isRunning || runningAction || runningTool}
            loading={entry.action === "start"}
            onClick={() => onAction("start")}
          >
            Start Hermes
          </Button>
          <Button
            disabled={!isRunning || runningAction || runningTool}
            loading={entry.action === "restart"}
            onClick={() => setConfirm("restart")}
          >
            Restart
          </Button>
          <Button
            variant="ghost"
            disabled={!isRunning || runningAction || runningTool}
            loading={entry.action === "restartNow"}
            onClick={() => setConfirm("restartNow")}
          >
            Restart now
          </Button>
          <Button
            variant="danger"
            disabled={!isRunning || runningAction || runningTool}
            loading={entry.action === "stop"}
            onClick={() => setConfirm("stop")}
          >
            Stop
          </Button>
          {!compact && (
            <>
              <Button
                variant="ghost"
                disabled={runningAction || runningTool}
                loading={entry.tool === "doctor"}
                onClick={() => onTool("doctor")}
              >
                Doctor
              </Button>
              <Button
                variant="ghost"
                disabled={runningAction || runningTool}
                loading={entry.tool === "update"}
                onClick={() => setConfirmUpdate(true)}
              >
                Update
              </Button>
              <Button variant="ghost" disabled={runningAction || runningTool} onClick={onSettings}>
                Configure commands
              </Button>
            </>
          )}
        </div>
      </div>

      <ConfirmDialog
        open={confirm !== null}
        title={
          confirm === "stop"
            ? `Stop Hermes on ${title}?`
            : `${confirm === "restartNow" ? "Restart now" : "Restart"} Hermes on ${title}?`
        }
        confirmLabel={
          confirm === "stop"
            ? "Stop Hermes"
            : confirm === "restartNow"
              ? "Restart now"
              : "Restart Hermes"
        }
        danger={confirm === "stop"}
        onCancel={() => setConfirm(null)}
        onConfirm={() => confirm && onAction(confirm)}
      >
        {confirm === "stop"
          ? `Hermes on ${title} will stop processing messages.`
          : confirm === "restartNow"
            ? `The gateway on ${title} will restart immediately; active work may be interrupted.`
            : `The gateway on ${title} will drain active work, then restart.`}
      </ConfirmDialog>
      <ConfirmDialog
        open={confirmUpdate}
        title={`Update Hermes on ${title}?`}
        confirmLabel="Update Hermes"
        danger
        onCancel={() => setConfirmUpdate(false)}
        onConfirm={() => onTool("update")}
      >
        The configured Hermes update command will run on <b>{title}</b>, then the gateway will
        restart.
      </ConfirmDialog>
    </Card>
  );
}
