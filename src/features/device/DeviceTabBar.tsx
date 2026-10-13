import { useEffect, useRef, useState } from "react";
import { ConfirmDialog } from "../../components/ConfirmDialog";
import { StatusDot } from "../../components/StatusDot";
import { deviceKey, useDevices } from "../../stores/devices";
import { useDeviceProfiles } from "../../stores/deviceProfiles";
import { useRoute } from "../../stores/route";
import type { AndroidDevice, ErrorPayload } from "../../types";
import { deviceStatus, deviceTitle } from "./deviceStatus";

// ─── persistence ─────────────────────────────────────────────────────────────

const STORAGE_KEY = "hacc.deviceTabOrder";

function loadTabOrder(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as string[]) : [];
  } catch {
    return [];
  }
}

function saveTabOrder(order: string[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(order));
  } catch {
    // Storage might be unavailable; silently ignore.
  }
}

/** Merge the persisted order with the current device keys.
 *  - Known keys keep their saved position.
 *  - New keys are appended in device-list order.
 *  - Keys no longer present in the device list are dropped. */
export function mergeTabOrder(saved: string[], current: string[]): string[] {
  const set = new Set(current);
  const ordered = saved.filter((k) => set.has(k));
  const remaining = current.filter((k) => !ordered.includes(k));
  return [...ordered, ...remaining];
}

// ─── overflow menu ────────────────────────────────────────────────────────────

interface OverflowMenuProps {
  device: AndroidDevice;
  onDisconnect: (d: AndroidDevice) => void;
  onRetry: (d: AndroidDevice) => void;
  onOpenProfile: (key: string) => void;
}

function OverflowMenu({ device, onDisconnect, onRetry, onOpenProfile }: OverflowMenuProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function onOutside(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", onOutside);
    return () => document.removeEventListener("mousedown", onOutside);
  }, [open]);

  const key = deviceKey(device);
  const canDisconnect = device.connection !== "usb" && device.state !== "disconnected";
  const canRetry = device.state === "disconnected";

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        aria-label={`More options for ${deviceTitle(device)}`}
        aria-haspopup="menu"
        aria-expanded={open}
        className="flex h-5 w-5 items-center justify-center rounded hover:bg-surface-2 text-muted text-xs leading-none"
        onClick={(e) => {
          e.stopPropagation();
          setOpen((o) => !o);
        }}
      >
        ⋯
      </button>
      {open && (
        <div
          role="menu"
          aria-label={`Options for ${deviceTitle(device)}`}
          className="absolute right-0 top-6 z-50 min-w-[140px] rounded-md border border-border bg-surface shadow-lg"
        >
          {canRetry && (
            <button
              role="menuitem"
              type="button"
              className="flex w-full items-center px-3 py-2 text-left text-sm hover:bg-surface-2"
              onClick={(e) => {
                e.stopPropagation();
                setOpen(false);
                onRetry(device);
              }}
            >
              Retry connection
            </button>
          )}
          {canDisconnect && (
            <button
              role="menuitem"
              type="button"
              className="flex w-full items-center px-3 py-2 text-left text-sm hover:bg-surface-2"
              onClick={(e) => {
                e.stopPropagation();
                setOpen(false);
                onDisconnect(device);
              }}
            >
              Disconnect
            </button>
          )}
          <button
            role="menuitem"
            type="button"
            className="flex w-full items-center px-3 py-2 text-left text-sm hover:bg-surface-2"
            onClick={(e) => {
              e.stopPropagation();
              setOpen(false);
              onOpenProfile(key);
            }}
          >
            Open profile
          </button>
        </div>
      )}
    </div>
  );
}

// ─── main component ───────────────────────────────────────────────────────────

interface DeviceTabBarProps {
  /** Called when the user opens a device's profile from the overflow menu. */
  onOpenProfile?: (key: string) => void;
}

export function DeviceTabBar({ onOpenProfile }: DeviceTabBarProps) {
  const { devices, activeKey, reconnect, disconnect, retry } = useDevices();
  const selectTab = useRoute((s) => s.selectTab);
  const profiles = useDeviceProfiles((s) => s.profiles);

  const [tabOrder, setTabOrder] = useState<string[]>(loadTabOrder);
  const [syncedOrder, setSyncedOrder] = useState<string | null>(null);
  const [confirmDisconnect, setConfirmDisconnect] = useState<AndroidDevice | null>(null);
  const [actionError, setActionError] = useState<ErrorPayload | null>(null);

  const allKeys = Object.keys(devices);
  const orderedKeys = mergeTabOrder(tabOrder, allKeys);

  // Persist the merged order as it changes. Render-phase adjustment (the
  // React-recommended alternative to setState in an effect).
  const orderSig = orderedKeys.join(",");
  if (syncedOrder !== orderSig) {
    setSyncedOrder(orderSig);
    if (orderedKeys.length > 0) {
      saveTabOrder(orderedKeys);
      setTabOrder(orderedKeys);
    }
  }

  // Only show the tab bar when there are multiple devices.
  if (allKeys.length < 2) return null;

  async function handleRetry(d: AndroidDevice) {
    setActionError(null);
    try {
      await retry(d.serial);
    } catch (e) {
      setActionError(e as ErrorPayload);
    }
  }

  async function handleDisconnect(d: AndroidDevice) {
    setActionError(null);
    try {
      await disconnect(d.serial);
    } catch (e) {
      setActionError(e as ErrorPayload);
    }
  }

  return (
    <div className="flex flex-col gap-1">
      {/* Tab strip */}
      <div
        role="tablist"
        aria-label="Connected devices"
        className="flex items-stretch gap-1 overflow-x-auto border-b border-border bg-surface pb-0 px-2 pt-1"
      >
        {orderedKeys.map((key) => {
          const d = devices[key];
          if (!d) return null;
          const profile = profiles[key] ?? profiles[d.deviceId ?? ""];
          const alias = profile?.alias ?? null;
          const color = profile?.color ?? null;
          const st = deviceStatus(d, reconnect[d.serial]);
          const isActive = key === activeKey;

          return (
            <div
              key={key}
              role="tab"
              aria-selected={isActive}
              className={`group flex min-w-0 shrink-0 cursor-pointer items-center gap-1.5 rounded-t-md border-x border-t px-2 pb-2 pt-1.5 text-sm transition-colors ${
                isActive
                  ? "border-border bg-bg text-text"
                  : "border-transparent bg-transparent text-muted hover:border-border/50 hover:text-text"
              }`}
              onClick={() => selectTab(key)}
            >
              {/* Color swatch */}
              <span
                className="inline-block h-3 w-3 shrink-0 rounded-full border border-border"
                style={color ? { backgroundColor: color } : { backgroundColor: "transparent" }}
                aria-hidden
              />
              {/* Label */}
              <span className="truncate max-w-[120px]">{alias ?? deviceTitle(d)}</span>
              {/* State badge */}
              <StatusDot tone={st.tone} label={st.label} pulse={st.pulse} />
              {/* Overflow menu */}
              <OverflowMenu
                device={d}
                onDisconnect={(d) => setConfirmDisconnect(d)}
                onRetry={handleRetry}
                onOpenProfile={onOpenProfile ?? (() => {})}
              />
            </div>
          );
        })}
      </div>

      {/* Inline action error */}
      {actionError && (
        <p className="px-2 text-xs text-danger" role="alert">
          {actionError.message}
        </p>
      )}

      {/* Disconnect confirm */}
      <ConfirmDialog
        open={confirmDisconnect !== null}
        title={`Disconnect ${confirmDisconnect ? (profiles[deviceKey(confirmDisconnect)]?.alias ?? deviceTitle(confirmDisconnect)) : "device"}?`}
        confirmLabel="Disconnect"
        danger
        onCancel={() => setConfirmDisconnect(null)}
        onConfirm={() => {
          const d = confirmDisconnect;
          setConfirmDisconnect(null);
          if (d) void handleDisconnect(d);
        }}
      >
        <p>
          Disconnect{" "}
          <b>
            {confirmDisconnect
              ? (profiles[deviceKey(confirmDisconnect)]?.alias ?? deviceTitle(confirmDisconnect))
              : ""}
          </b>
          {confirmDisconnect?.model ? ` (${confirmDisconnect.model})` : ""}
          {confirmDisconnect?.ipAddress ? ` · ${confirmDisconnect.ipAddress}` : ""}?
        </p>
        <p className="mt-2 text-muted text-sm">Automatic reconnection will be disabled for it.</p>
      </ConfirmDialog>
    </div>
  );
}

// ─── persistence helpers (exported for tests) ─────────────────────────────────

export { loadTabOrder, saveTabOrder };
