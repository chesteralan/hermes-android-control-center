import { useEffect } from "react";
import { Button } from "../../components/Button";
import { Card } from "../../components/Card";
import { ErrorPanel } from "../../components/ErrorPanel";
import { KeyValue } from "../../components/KeyValue";
import { StatusDot } from "../../components/StatusDot";
import { formatBytes, orUnknown, UNKNOWN } from "../../lib/format";
import { deviceKey, useDevices } from "../../stores/devices";
import type { AndroidDevice, DeviceInfo, TermuxPackageInfo } from "../../types";
import { deviceStatus, deviceTitle } from "./deviceStatus";

const REFRESH_MS = 30_000;

function termuxLabel(t: TermuxPackageInfo | null): string {
  if (!t) return UNKNOWN;
  if (!t.installed) return "Not installed";
  const src = {
    playStore: "Google Play (outdated — reinstall from F-Droid/GitHub)",
    fDroid: "F-Droid",
    sideloaded: "GitHub/sideloaded",
    unknown: "unknown source",
  }[t.source];
  return `${t.versionName ?? "installed"} · ${src}`;
}

export function infoRows(d: AndroidDevice, info: DeviceInfo | null): Array<[string, string]> {
  const b = info?.battery;
  const battery =
    b?.level != null
      ? `${b.level}%${b.charging ? " · charging" : b.status === "Full" ? " · full" : ""}`
      : UNKNOWN;
  const storage = info?.storage
    ? `${formatBytes(info.storage.freeBytes)} free of ${formatBytes(info.storage.totalBytes)}`
    : UNKNOWN;
  const memory = info?.memory
    ? `${formatBytes(info.memory.totalBytes - info.memory.availableBytes)} / ${formatBytes(info.memory.totalBytes)}`
    : UNKNOWN;
  const cpu = info?.cpu
    ? [info.cpu.hardware, info.cpu.cores ? `${info.cpu.cores} cores` : null, info.cpu.abi]
        .filter(Boolean)
        .join(" · ")
    : UNKNOWN;
  return [
    ["Model", orUnknown(info?.model ?? d.model)],
    ["Manufacturer", orUnknown(info?.manufacturer)],
    ["Android", orUnknown(info?.androidVersion)],
    ["SDK", orUnknown(info?.sdk)],
    ["Serial", orUnknown(info?.deviceId ?? d.deviceId)],
    ["ADB", d.serial],
    ["IP", orUnknown(info?.ipAddress ?? d.ipAddress)],
    ["Battery", battery],
    ["Storage", storage],
    ["Memory", memory],
    ["CPU", cpu || UNKNOWN],
    ["Termux", termuxLabel(info?.termux ?? null)],
  ];
}

export function DeviceInfoCard({ device }: { device: AndroidDevice }) {
  const key = deviceKey(device);
  const entry = useDevices((s) => s.info[key]);
  const reconnect = useDevices((s) => s.reconnect[device.serial]);
  const loadInfo = useDevices((s) => s.loadInfo);
  const ready = device.state === "device";

  useEffect(() => {
    if (!ready) return;
    void loadInfo(key);
    const t = setInterval(() => void loadInfo(key), REFRESH_MS);
    return () => clearInterval(t);
  }, [key, ready, loadInfo]);

  const st = deviceStatus(device, reconnect);
  return (
    <Card
      title="Android device"
      actions={
        <Button
          variant="ghost"
          onClick={() => loadInfo(key)}
          loading={entry?.loading}
          disabled={!ready}
        >
          Refresh
        </Button>
      }
    >
      <div className="mb-3 flex flex-col gap-1">
        <StatusDot tone={st.tone} label={st.label} pulse={st.pulse} />
        <span className="text-[18px] font-semibold">{deviceTitle(device)}</span>
        {st.hint && <span className="text-muted">{st.hint}</span>}
      </div>
      <KeyValue rows={infoRows(device, entry?.data ?? null)} />
      {entry?.error && (
        <div className="mt-3">
          <ErrorPanel error={entry.error} onRetry={() => loadInfo(key)} />
        </div>
      )}
    </Card>
  );
}
