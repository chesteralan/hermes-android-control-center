import { useEffect } from "react";
import { Button } from "../../components/Button";
import { Card } from "../../components/Card";
import { ErrorPanel } from "../../components/ErrorPanel";
import { StatusDot } from "../../components/StatusDot";
import { deviceKey, useDevices } from "../../stores/devices";
import { useDeviceProfiles } from "../../stores/deviceProfiles";
import { useHermes } from "../../stores/hermes";
import { isGlobalRoute, useRoute } from "../../stores/route";
import type { AndroidDevice } from "../../types";
import { NoDeviceState } from "../device/NoDeviceState";
import { deviceStatus, deviceTitle } from "../device/deviceStatus";

/** Same cadence as the per-device Hermes status polling. */
const POLL_MS = 10_000;

function formatBytes(bytes: number): string {
  const gb = bytes / 1024 ** 3;
  return gb >= 1 ? `${gb.toFixed(1)} GB` : `${Math.round(bytes / 1024 ** 2)} MB`;
}

function OverviewCard({ device, onOpen }: { device: AndroidDevice; onOpen: () => void }) {
  const key = deviceKey(device);
  const profile = useDeviceProfiles((s) =>
    device.deviceId ? (s.profiles[device.deviceId] ?? null) : null,
  );
  const reconnect = useDevices((s) => s.reconnect[key]);
  const info = useDevices((s) => s.info[key]);
  const loadInfo = useDevices((s) => s.loadInfo);
  const hermes = useHermes((s) => s.byDevice[key]);
  const hermesRefresh = useHermes((s) => s.refresh);

  // Each card polls independently so one slow phone never gates another (M9-T6).
  useEffect(() => {
    void loadInfo(key);
    void hermesRefresh(key, device.serial);
    const timer = setInterval(() => {
      void loadInfo(key);
      void hermesRefresh(key, device.serial);
    }, POLL_MS);
    return () => clearInterval(timer);
  }, [device.serial, hermesRefresh, key, loadInfo]);

  const st = deviceStatus(device, reconnect);
  const battery = info?.data?.battery ?? null;
  const storage = info?.data?.storage ?? null;
  const gateway = hermes?.status?.gateway ?? null;

  return (
    <Card
      title={
        <span className="flex items-center gap-2 normal-case tracking-normal text-text">
          <span
            className="inline-block h-2.5 w-2.5 shrink-0 rounded-full border border-border"
            style={profile?.color ? { backgroundColor: profile.color } : undefined}
            aria-hidden
          />
          {profile?.alias ?? deviceTitle(device)}
        </span>
      }
      actions={
        <Button variant="ghost" onClick={onOpen}>
          Open
        </Button>
      }
    >
      <div className="flex flex-col gap-1.5 text-[12px]">
        <span className="text-muted">{device.model ?? device.serial}</span>
        <StatusDot tone={st.tone} label={st.label} pulse={st.pulse} />
        <span>
          Battery:{" "}
          {battery ? (
            <>
              {battery.level ?? "?"}%{battery.charging ? " · charging" : ""}
            </>
          ) : info?.loading ? (
            <span className="text-muted">loading…</span>
          ) : info?.error ? (
            <span className="text-danger">no info</span>
          ) : (
            <span className="text-muted">unknown</span>
          )}
        </span>
        <span>
          Storage:{" "}
          {storage ? (
            <>
              {formatBytes(storage.freeBytes)} free of {formatBytes(storage.totalBytes)}
            </>
          ) : info?.loading ? (
            <span className="text-muted">loading…</span>
          ) : (
            <span className="text-muted">unknown</span>
          )}
        </span>
        <span>
          Hermes gateway:{" "}
          {hermes?.loadingStatus ? (
            <span className="text-muted">checking…</span>
          ) : gateway ? (
            gateway
          ) : hermes?.statusError ? (
            <span className="text-danger">unavailable</span>
          ) : (
            <span className="text-muted">unknown</span>
          )}
        </span>
        {reconnect && (
          <span className="text-warning">
            Reconnecting (attempt {reconnect.attempt}/{reconnect.maxAttempts})
          </span>
        )}
        {info?.error && <ErrorPanel error={info.error} onRetry={() => void loadInfo(key)} />}
      </div>
    </Card>
  );
}

export function OverviewView() {
  const devices = useDevices((s) => s.devices);
  const route = useRoute((s) => s.route);
  const selectTab = useRoute((s) => s.selectTab);
  const go = useRoute((s) => s.go);
  const list = Object.values(devices);

  if (list.length === 0) return <NoDeviceState />;

  return (
    <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
      {list.map((device) => {
        const key = deviceKey(device);
        return (
          <OverviewCard
            key={key}
            device={device}
            onOpen={() => {
              selectTab(key);
              if (isGlobalRoute(route)) go("device");
            }}
          />
        );
      })}
    </div>
  );
}
