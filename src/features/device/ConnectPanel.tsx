import { useState, type FormEvent } from "react";
import { Button } from "../../components/Button";
import { ErrorPanel } from "../../components/ErrorPanel";
import { validateHost, validatePort } from "../../lib/format";
import { ipc } from "../../lib/ipc";
import { useDevices } from "../../stores/devices";
import { useToasts } from "../../stores/toast";
import { useUi } from "../../stores/ui";
import type { ErrorPayload, MdnsService } from "../../types";

export function ConnectPanel() {
  const connect = useDevices((s) => s.connect);
  const openPair = useUi((s) => s.openPair);
  const toast = useToasts((s) => s.push);
  const [host, setHost] = useState("");
  const [port, setPort] = useState("");
  const [touched, setTouched] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ErrorPayload | null>(null);
  const [services, setServices] = useState<MdnsService[] | null>(null);
  const [discovering, setDiscovering] = useState(false);

  const hostErr = validateHost(host);
  const portErr = validatePort(port);

  async function doConnect(address: string) {
    setBusy(true);
    setError(null);
    try {
      const d = await connect(address);
      toast(`Connected to ${d.model ?? address}`, "success");
    } catch (e) {
      setError(e as ErrorPayload);
    } finally {
      setBusy(false);
    }
  }

  function onSubmit(e: FormEvent) {
    e.preventDefault();
    setTouched(true);
    if (hostErr || portErr) return;
    void doConnect(`${host.trim()}:${port.trim()}`);
  }

  async function discover() {
    setDiscovering(true);
    setError(null);
    try {
      setServices((await ipc.discoverDevices()).filter((s) => s.kind === "connect"));
    } catch (e) {
      setError(e as ErrorPayload);
    } finally {
      setDiscovering(false);
    }
  }

  return (
    <div className="flex flex-col gap-3">
      <form onSubmit={onSubmit} className="flex flex-wrap items-start gap-2" noValidate>
        <label className="flex flex-col gap-1">
          <span className="text-muted">IP address</span>
          <input
            aria-label="IP address"
            className="w-44 rounded-md border border-border bg-bg px-2 py-1.5 font-mono"
            placeholder="192.168.1.25"
            value={host}
            onChange={(e) => setHost(e.target.value)}
          />
          {touched && hostErr && <span className="text-danger">{hostErr}</span>}
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-muted">Port</span>
          <input
            aria-label="Port"
            className="w-24 rounded-md border border-border bg-bg px-2 py-1.5 font-mono"
            placeholder="37145"
            inputMode="numeric"
            value={port}
            onChange={(e) => setPort(e.target.value)}
          />
          {touched && portErr && <span className="text-danger">{portErr}</span>}
        </label>
        <div className="flex gap-2 pt-[22px]">
          <Button type="submit" variant="primary" loading={busy}>
            Connect
          </Button>
          <Button onClick={() => openPair("qr")}>Pair new device</Button>
          <Button onClick={discover} loading={discovering}>
            Discover
          </Button>
        </div>
      </form>
      <p className="text-muted">
        Use the <b>IP address &amp; Port</b> from Wireless debugging. The pairing port is different.
      </p>
      {error && <ErrorPanel error={error} />}
      {services && (
        <div className="rounded-md border border-border">
          {services.length === 0 ? (
            <p className="p-3 text-muted">
              No phones found on the network. Connect with IP and port instead.
            </p>
          ) : (
            services.map((s) => (
              <div
                key={s.address}
                className="flex items-center justify-between border-b border-border px-3 py-2 last:border-0"
              >
                <span className="font-mono">
                  {s.name} <span className="text-muted">{s.address}</span>
                </span>
                <Button onClick={() => doConnect(s.address)}>Connect</Button>
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}
