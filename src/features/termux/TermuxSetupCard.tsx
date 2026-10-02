import { useEffect, useState } from "react";
import { Button } from "../../components/Button";
import { Card } from "../../components/Card";
import { ErrorPanel } from "../../components/ErrorPanel";
import { StatusDot, type Tone } from "../../components/StatusDot";
import { ipc } from "../../lib/ipc";
import { useToasts } from "../../stores/toast";
import type { AndroidDevice, CheckStatus, ErrorPayload, TermuxCheck } from "../../types";

const tone: Record<CheckStatus, Tone> = { ok: "success", failed: "danger", skipped: "muted" };
const label: Record<CheckStatus, string> = { ok: "OK", failed: "Failed", skipped: "Skipped" };

export function setupSteps(publicKey: string): Array<{ title: string; command: string }> {
  const q = `'${publicKey.replace(/'/g, `'\\''`)}'`;
  return [
    { title: "Install OpenSSH", command: "pkg install -y openssh" },
    {
      title: "Authorize this app's key",
      command: `mkdir -p ~/.ssh && chmod 700 ~/.ssh && (grep -qxF ${q} ~/.ssh/authorized_keys 2>/dev/null || echo ${q} >> ~/.ssh/authorized_keys) && chmod 600 ~/.ssh/authorized_keys`,
    },
    {
      title: "Listen on the phone only (127.0.0.1)",
      command:
        "grep -q '^ListenAddress 127.0.0.1' $PREFIX/etc/ssh/sshd_config || echo 'ListenAddress 127.0.0.1' >> $PREFIX/etc/ssh/sshd_config",
    },
    { title: "Start sshd", command: "pkill sshd; sshd" },
    {
      title: "Optional: keep running in background",
      command: "termux-wake-lock; pkg install -y termux-services && sv-enable sshd",
    },
  ];
}

function CopyRow({ title, command }: { title: string; command: string }) {
  const toast = useToasts((s) => s.push);
  return (
    <li className="flex flex-col gap-1">
      <span>{title}</span>
      <div className="flex items-start gap-2">
        <code className="flex-1 overflow-x-auto whitespace-pre rounded bg-bg px-2 py-1.5 text-[12px]">
          {command}
        </code>
        <Button
          variant="ghost"
          onClick={async () => {
            await navigator.clipboard?.writeText(command);
            toast("Copied", "success");
          }}
        >
          Copy
        </Button>
      </div>
    </li>
  );
}

export function TermuxSetupCard({ device }: { device: AndroidDevice }) {
  const [publicKey, setPublicKey] = useState<string | null>(null);
  const [check, setCheck] = useState<TermuxCheck | null>(null);
  const [error, setError] = useState<ErrorPayload | null>(null);
  const [busy, setBusy] = useState(false);
  const ready = device.state === "device";

  useEffect(() => {
    ipc
      .getTermuxPublicKey()
      .then(setPublicKey)
      .catch((e: ErrorPayload) => setError(e));
  }, []);

  async function verify() {
    setBusy(true);
    setError(null);
    try {
      setCheck(await ipc.checkTermux(device.serial));
    } catch (e) {
      setError(e as ErrorPayload);
    } finally {
      setBusy(false);
    }
  }

  async function forget() {
    await ipc.forgetTermuxHostKey(device.serial);
    await verify();
  }

  const sshFailed = check?.items.some((i) => i.id === "ssh" && i.status === "failed");

  return (
    <Card
      title="Termux bridge"
      actions={
        <Button variant="primary" onClick={verify} loading={busy} disabled={!ready}>
          Verify
        </Button>
      }
    >
      {check?.ready ? (
        <p className="mb-3">
          <StatusDot tone="success" label="Ready — commands can run inside Termux" />
        </p>
      ) : (
        <>
          <p className="mb-3 text-muted">
            The app reaches Termux over SSH through ADB. Nothing is exposed on Wi-Fi. Run these once
            in the Termux app on the phone, then press Verify.
          </p>
          {publicKey && (
            <ol className="mb-3 flex list-decimal flex-col gap-3 pl-5">
              {setupSteps(publicKey).map((s) => (
                <CopyRow key={s.title} {...s} />
              ))}
            </ol>
          )}
        </>
      )}
      {check && (
        <ul className="flex flex-col gap-1.5" aria-label="Termux checks">
          {check.items.map((i) => (
            <li key={i.id} className="flex flex-col">
              <span className="flex items-center gap-3">
                <StatusDot tone={tone[i.status]} label={`${i.label}: ${label[i.status]}`} />
                {i.detail && <span className="font-mono text-[12px] text-muted">{i.detail}</span>}
              </span>
              {i.hint && <span className="pl-4 text-[12px] text-warning">{i.hint}</span>}
            </li>
          ))}
        </ul>
      )}
      {sshFailed && (
        <Button variant="ghost" className="mt-2" onClick={forget}>
          Forget pinned host key (after reinstalling Termux)
        </Button>
      )}
      {error && <ErrorPanel error={error} />}
    </Card>
  );
}
