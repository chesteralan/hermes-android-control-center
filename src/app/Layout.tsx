import { useEffect, type ReactNode } from "react";
import { StatusDot } from "../components/StatusDot";
import { deviceStatus, deviceTitle } from "../features/device/deviceStatus";
import { isModKey, modLabel } from "../lib/platform";
import { useActiveDevice, useDevices } from "../stores/devices";
import { ROUTES, ROUTE_LABELS, useRoute } from "../stores/route";

export function Layout({ children }: { children: ReactNode }) {
  const { route, go } = useRoute();
  const device = useActiveDevice();
  const reconnect = useDevices((s) => (device ? s.reconnect[device.serial] : undefined));

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!isModKey(e)) return;
      const n = Number(e.key);
      const target = ROUTES[n - 1];
      if (target) {
        e.preventDefault();
        go(target);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [go]);

  const st = device ? deviceStatus(device, reconnect) : null;
  return (
    <div className="grid h-full grid-cols-[200px_1fr] grid-rows-[44px_1fr]">
      <header className="col-span-2 flex items-center justify-between border-b border-border bg-surface px-4">
        <span className="font-semibold tracking-wide">Hermes Control Center</span>
        <span className="flex items-center gap-4 text-muted">
          {device && st ? (
            <span className="flex items-center gap-2">
              <span className="text-text">{deviceTitle(device)}</span>
              <StatusDot tone={st.tone} label={st.label} pulse={st.pulse} />
            </span>
          ) : (
            <span>No phone selected</span>
          )}
          <button type="button" className="hover:text-text" onClick={() => go("settings")}>
            Settings
          </button>
        </span>
      </header>
      <nav className="border-r border-border bg-surface py-2" aria-label="Main">
        {ROUTES.map((r, i) => (
          <button
            key={r}
            type="button"
            aria-current={route === r ? "page" : undefined}
            onClick={() => go(r)}
            className={`flex w-full items-center justify-between px-4 py-2 text-left ${
              route === r ? "bg-accent/10 text-text" : "text-muted hover:text-text"
            }`}
          >
            <span>{ROUTE_LABELS[r]}</span>
            <span className="text-[11px] opacity-60">
              {modLabel}
              {i + 1}
            </span>
          </button>
        ))}
      </nav>
      <main className="overflow-auto">{children}</main>
    </div>
  );
}
