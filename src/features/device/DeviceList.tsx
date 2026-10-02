import { useState } from "react";
import { Button } from "../../components/Button";
import { ConfirmDialog } from "../../components/ConfirmDialog";
import { ErrorPanel } from "../../components/ErrorPanel";
import { StatusDot } from "../../components/StatusDot";
import { deviceKey, useDevices } from "../../stores/devices";
import type { AndroidDevice, ErrorPayload } from "../../types";
import { connectionLabel, deviceStatus, deviceTitle } from "./deviceStatus";

export function DeviceList() {
  const { devices, activeKey, reconnect, select, disconnect, retry } = useDevices();
  const [confirm, setConfirm] = useState<AndroidDevice | null>(null);
  const [error, setError] = useState<ErrorPayload | null>(null);
  const list = Object.values(devices);

  async function run(fn: () => Promise<void>) {
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(e as ErrorPayload);
    }
  }

  return (
    <div className="flex flex-col gap-2">
      {list.map((d) => {
        const st = deviceStatus(d, reconnect[d.serial]);
        const key = deviceKey(d);
        return (
          <div
            key={key}
            className={`flex items-center justify-between rounded-md border px-3 py-2 ${key === activeKey ? "border-accent/60 bg-accent/5" : "border-border"}`}
          >
            <button
              type="button"
              className="flex flex-1 flex-col items-start text-left"
              onClick={() => select(key)}
            >
              <span className="font-semibold">
                {deviceTitle(d)}{" "}
                <span className="ml-1 rounded bg-surface-2 px-1.5 text-[11px] text-muted">
                  {connectionLabel(d)}
                </span>
              </span>
              <span className="font-mono text-[12px] text-muted">{d.serial}</span>
              <StatusDot tone={st.tone} label={st.label} pulse={st.pulse} />
              {st.hint && <span className="text-[12px] text-muted">{st.hint}</span>}
            </button>
            <div className="flex gap-2">
              {d.state === "disconnected" && (
                <Button onClick={() => run(() => retry(d.serial))}>Retry</Button>
              )}
              {d.connection !== "usb" && d.state !== "disconnected" && (
                <Button variant="ghost" onClick={() => setConfirm(d)}>
                  Disconnect
                </Button>
              )}
            </div>
          </div>
        );
      })}
      {error && <ErrorPanel error={error} />}
      <ConfirmDialog
        open={confirm !== null}
        title="Disconnect device?"
        confirmLabel="Disconnect"
        danger
        onCancel={() => setConfirm(null)}
        onConfirm={() => {
          const d = confirm;
          setConfirm(null);
          if (d) void run(() => disconnect(d.serial));
        }}
      >
        Disconnect <b>{confirm ? deviceTitle(confirm) : ""}</b> ({confirm?.serial})? Automatic
        reconnection will be turned off for it.
      </ConfirmDialog>
    </div>
  );
}
