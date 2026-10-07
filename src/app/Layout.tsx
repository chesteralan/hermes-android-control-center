import { useEffect, useState, type ReactNode } from "react";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { StatusDot } from "../components/StatusDot";
import { Dialog } from "../components/Dialog";
import { deviceStatus, deviceTitle } from "../features/device/deviceStatus";
import { ipc } from "../lib/ipc";
import { isModKey, modLabel } from "../lib/platform";
import { useActiveDevice, useDevices } from "../stores/devices";
import { ROUTES, ROUTE_LABELS, useRoute } from "../stores/route";

export function Layout({ children }: { children: ReactNode }) {
  const { route, go } = useRoute();
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [aboutOpen, setAboutOpen] = useState(false);
  const [availableUpdate, setAvailableUpdate] = useState<Update | null>(null);
  const [checkingForUpdates, setCheckingForUpdates] = useState(false);
  const [installingUpdate, setInstallingUpdate] = useState(false);
  const [updateMessage, setUpdateMessage] = useState("");
  const device = useActiveDevice();
  const reconnect = useDevices((s) => (device ? s.reconnect[device.serial] : undefined));

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!isModKey(e)) return;
      if (e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((open) => !open);
        setQuery("");
        return;
      }
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

  useEffect(() => {
    const unlisten = ipc.onMenuCommand((command) => {
      if (command === "settings") go("settings");
      if (command === "about") setAboutOpen(true);
      if (command === "checkForUpdates") {
        setAboutOpen(true);
        void checkForUpdates();
      }
    });
    return () => {
      void unlisten.then((stopListening) => stopListening());
    };
  }, [go]);

  const st = device ? deviceStatus(device, reconnect) : null;

  async function checkForUpdates(): Promise<void> {
    setCheckingForUpdates(true);
    setUpdateMessage("");
    try {
      if (!(await ipc.supportsInAppUpdates())) {
        setUpdateMessage(
          "This installation is managed by your Linux package manager. Update it with apt, dnf, or your package installer.",
        );
        return;
      }
      const update = await check();
      if (update) {
        setAvailableUpdate(update);
        setUpdateMessage("");
      } else {
        setUpdateMessage("You're up to date.");
      }
    } catch (error) {
      setUpdateMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setCheckingForUpdates(false);
    }
  }

  async function closeUpdatePrompt(): Promise<void> {
    await availableUpdate?.close();
    setAvailableUpdate(null);
  }

  async function installUpdate(): Promise<void> {
    if (!availableUpdate || installingUpdate) return;
    setInstallingUpdate(true);
    try {
      await availableUpdate.downloadAndInstall();
      await availableUpdate.close();
      setAvailableUpdate(null);
      await relaunch();
    } catch (error) {
      setUpdateMessage(error instanceof Error ? error.message : String(error));
      await availableUpdate.close();
      setAvailableUpdate(null);
    } finally {
      setInstallingUpdate(false);
    }
  }

  return (
    <div className="grid h-full grid-cols-[200px_1fr] grid-rows-[44px_1fr]">
      <header className="col-span-2 flex items-center justify-between border-b border-border bg-surface px-4">
        <span className="flex items-baseline gap-2">
          <span className="font-semibold tracking-wide">Hermes Control Center</span>
          <span className="text-xs text-muted">v{__APP_VERSION__}</span>
        </span>
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
          <button type="button" className="hover:text-text" onClick={() => setAboutOpen(true)}>
            About
          </button>
          <button
            type="button"
            aria-keyshortcuts="Meta+K Control+K"
            className="rounded border border-border px-2 py-1 hover:text-text"
            onClick={() => {
              setPaletteOpen(true);
              setQuery("");
            }}
          >
            Commands <span className="text-muted">{modLabel}K</span>
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
      <Dialog open={paletteOpen} title="Command palette" onClose={() => setPaletteOpen(false)}>
        <input
          aria-label="Search views and commands"
          className="w-full border-b border-border bg-transparent px-4 py-3 outline-none"
          placeholder="Search views..."
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <ul className="max-h-72 overflow-auto p-2" aria-label="Available views">
          {ROUTES.filter((candidate) =>
            ROUTE_LABELS[candidate].toLowerCase().includes(query.trim().toLowerCase()),
          ).map((candidate) => (
            <li key={candidate}>
              <button
                type="button"
                className="flex w-full items-center justify-between rounded px-3 py-2 text-left hover:bg-surface-2"
                onClick={() => {
                  go(candidate);
                  setPaletteOpen(false);
                }}
              >
                <span>{ROUTE_LABELS[candidate]}</span>
                <kbd className="text-muted">
                  {modLabel}
                  {ROUTES.indexOf(candidate) + 1}
                </kbd>
              </button>
            </li>
          ))}
          {ROUTES.every(
            (candidate) =>
              !ROUTE_LABELS[candidate].toLowerCase().includes(query.trim().toLowerCase()),
          ) && <li className="px-3 py-2 text-muted">No matching views</li>}
        </ul>
        <footer className="border-t border-border px-4 py-2 text-muted">
          View shortcuts:{" "}
          {ROUTES.map(
            (candidate, index) => `${modLabel}${index + 1} ${ROUTE_LABELS[candidate]}`,
          ).join(" · ")}
        </footer>
      </Dialog>
      <Dialog
        open={aboutOpen}
        title="About Hermes Control Center"
        onClose={() => setAboutOpen(false)}
      >
        <p className="font-medium">Hermes Control Center v{__APP_VERSION__}</p>
        <p className="mt-2 text-muted">
          Desktop controls for Hermes Agent on Android through Wireless ADB and Termux.
        </p>
        <div className="mt-4 flex items-center gap-3">
          <button
            type="button"
            className="rounded border border-border px-3 py-1.5 hover:text-text"
            onClick={() => void checkForUpdates()}
            disabled={checkingForUpdates}
          >
            {checkingForUpdates ? "Checking..." : "Check for Updates"}
          </button>
          {updateMessage && (
            <span role="status" className="text-muted">
              {updateMessage}
            </span>
          )}
        </div>
        <button
          type="button"
          className="mt-4 rounded border border-border px-3 py-1.5 hover:text-text"
          onClick={() => setAboutOpen(false)}
        >
          Close
        </button>
      </Dialog>
      <ConfirmDialog
        open={availableUpdate !== null}
        title={`Install version ${availableUpdate?.version ?? ""}?`}
        confirmLabel={installingUpdate ? "Installing..." : "Install update"}
        onConfirm={() => void installUpdate()}
        onCancel={() => void closeUpdatePrompt()}
      >
        <p>Current version: {availableUpdate?.currentVersion}</p>
        {availableUpdate?.body && (
          <p className="mt-3 whitespace-pre-wrap">{availableUpdate.body}</p>
        )}
      </ConfirmDialog>
    </div>
  );
}
