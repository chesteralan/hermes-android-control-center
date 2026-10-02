import { useEffect, useState, type FormEvent } from "react";
import { Button } from "../../components/Button";
import { Dialog } from "../../components/Dialog";
import { ErrorPanel } from "../../components/ErrorPanel";
import { formatSeconds, validateHost, validatePort } from "../../lib/format";
import { ipc } from "../../lib/ipc";
import { useDevices } from "../../stores/devices";
import { useToasts } from "../../stores/toast";
import { useUi, type PairTab } from "../../stores/ui";
import type { ErrorPayload, QrPairEvent } from "../../types";

export function PairDialog() {
  const tab = useUi((s) => s.pairTab);
  const openPair = useUi((s) => s.openPair);
  const close = useUi((s) => s.closePair);
  return (
    <Dialog open={tab !== null} title="Pair new device" onClose={close}>
      <div role="tablist" className="mb-4 flex gap-1 rounded-md bg-bg p-1">
        {(["qr", "code"] as PairTab[]).map((t) => (
          <button
            key={t}
            role="tab"
            type="button"
            aria-selected={tab === t}
            onClick={() => openPair(t)}
            className={`flex-1 rounded px-3 py-1.5 ${tab === t ? "bg-surface-2 text-text" : "text-muted"}`}
          >
            {t === "qr" ? "QR code" : "Pairing code"}
          </button>
        ))}
      </div>
      {tab === "qr" && <QrTab onUseCode={() => openPair("code")} onDone={close} />}
      {tab === "code" && <CodeTab onDone={close} />}
    </Dialog>
  );
}

function svgDataUrl(svg: string) {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

function describe(ev: QrPairEvent | null): string {
  if (!ev) return "Starting…";
  switch (ev.type) {
    case "waiting":
      return `Waiting for phone… (${formatSeconds(ev.remainingMs)} left)`;
    case "found":
      return `Found phone at ${ev.address}, pairing…`;
    case "paired":
      return "Paired. Connecting…";
    case "connected":
      return `Connected to ${ev.address}`;
    case "failed":
      return "Pairing failed";
    case "expired":
      return "QR code expired";
  }
}

export function QrTab({ onUseCode, onDone }: { onUseCode: () => void; onDone: () => void }) {
  const refresh = useDevices((s) => s.refresh);
  const toast = useToasts((s) => s.push);
  const [svg, setSvg] = useState<string | null>(null);
  const [event, setEvent] = useState<QrPairEvent | null>(null);
  const [error, setError] = useState<ErrorPayload | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let active = true;
    let sessionId: string | null = null;
    ipc
      .startQrPairing((ev) => {
        if (!active) return;
        setEvent(ev);
        if (ev.type === "failed") setError(ev.error);
        if (ev.type === "connected") {
          toast(`Paired and connected to ${ev.address}`, "success");
          void refresh();
          onDone();
        }
      })
      .then((s) => {
        if (!active) {
          void ipc.cancelQrPairing(s.sessionId);
          return;
        }
        sessionId = s.sessionId;
        setSvg(s.qrSvg);
      })
      .catch((e: ErrorPayload) => active && setError(e));
    return () => {
      active = false;
      if (sessionId) void ipc.cancelQrPairing(sessionId);
    };
  }, [attempt, onDone, refresh, toast]);

  const regenerate = () => {
    setSvg(null);
    setEvent(null);
    setError(null);
    setAttempt((a) => a + 1);
  };

  const ended = event?.type === "failed" || event?.type === "expired" || (error && !svg);
  return (
    <div className="flex flex-col items-center gap-3">
      {svg && !ended && (
        <img
          src={svgDataUrl(svg)}
          alt="Pairing QR code"
          className="h-60 w-60 rounded bg-white p-2"
        />
      )}
      <p className="text-center text-muted">
        On the phone: <b>Developer options → Wireless debugging → Pair device with QR code</b>, then
        scan.
      </p>
      <p role="status" className="font-mono">
        {describe(event)}
      </p>
      {error && <ErrorPanel error={error} />}
      <div className="flex gap-2">
        {ended && <Button onClick={regenerate}>Regenerate</Button>}
        <Button variant="ghost" onClick={onUseCode}>
          Use pairing code instead
        </Button>
      </div>
    </div>
  );
}

export function CodeTab({ onDone }: { onDone: () => void }) {
  const toast = useToasts((s) => s.push);
  const [host, setHost] = useState("");
  const [port, setPort] = useState("");
  const [code, setCode] = useState("");
  const [touched, setTouched] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ErrorPayload | null>(null);
  const errs = [
    validateHost(host),
    validatePort(port),
    /^[A-Za-z0-9]{6,}$/.test(code.trim()) ? null : "Enter the 6-digit code.",
  ];

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setTouched(true);
    if (errs.some(Boolean)) return;
    setBusy(true);
    setError(null);
    try {
      await ipc.pairDevice(`${host.trim()}:${port.trim()}`, code.trim());
      toast("Paired. Now connect using the IP address & Port shown on the phone.", "success");
      onDone();
    } catch (e) {
      setError(e as ErrorPayload);
    } finally {
      setBusy(false);
    }
  }

  const field = (
    label: string,
    value: string,
    set: (v: string) => void,
    err: string | null,
    ph: string,
  ) => (
    <label className="flex flex-col gap-1">
      <span className="text-muted">{label}</span>
      <input
        aria-label={label}
        className="rounded-md border border-border bg-bg px-2 py-1.5 font-mono"
        value={value}
        placeholder={ph}
        onChange={(e) => set(e.target.value)}
      />
      {touched && err && <span className="text-danger">{err}</span>}
    </label>
  );

  return (
    <form onSubmit={onSubmit} className="flex flex-col gap-3" noValidate>
      <p className="text-muted">
        On the phone: <b>Wireless debugging → Pair device with pairing code</b>.
      </p>
      {field("IP address", host, setHost, errs[0] ?? null, "192.168.1.25")}
      {field("Pairing port", port, setPort, errs[1] ?? null, "40123")}
      {field("Pairing code", code, setCode, errs[2] ?? null, "123456")}
      {error && <ErrorPanel error={error} />}
      <div className="flex justify-end">
        <Button type="submit" variant="primary" loading={busy}>
          Pair
        </Button>
      </div>
    </form>
  );
}
