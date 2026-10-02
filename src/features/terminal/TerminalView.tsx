import { useEffect, useRef, useState, type FormEvent } from "react";
import { Button } from "../../components/Button";
import { ErrorPanel } from "../../components/ErrorPanel";
import { deviceKey, useActiveDevice } from "../../stores/devices";
import { useTerminal, type CommandBlock } from "../../stores/terminal";
import { NoDeviceState } from "../device/NoDeviceState";
import { deviceTitle } from "../device/deviceStatus";

function Footer({ b }: { b: CommandBlock }) {
  if (b.running) return <span className="text-accent">running…</span>;
  if (b.cancelled) return <span className="text-warning">cancelled</span>;
  if (b.error) return null;
  const tone = b.exitCode === 0 ? "text-success" : "text-danger";
  return (
    <span className={tone}>
      exit {b.exitCode ?? "?"} · {b.durationMs ?? 0} ms
    </span>
  );
}

const NO_BLOCKS: CommandBlock[] = [];

export function TerminalView() {
  const device = useActiveDevice();
  const key = device ? deviceKey(device) : "";
  const blocks = useTerminal((s) => s.sessions[key]?.blocks ?? NO_BLOCKS);
  const { run, cancel, clear } = useTerminal();
  const [command, setCommand] = useState("");
  const bottom = useRef<HTMLDivElement>(null);
  const running = blocks.find((b) => b.running);
  const lineCount = blocks.reduce((n, b) => n + b.lines.length, 0);

  useEffect(() => {
    bottom.current?.scrollIntoView?.({ block: "end" });
  }, [blocks.length, lineCount]);

  if (!device) return <NoDeviceState />;

  function onSubmit(e: FormEvent) {
    e.preventDefault();
    const cmd = command.trim();
    if (!cmd || running || !device) return;
    setCommand("");
    void run(key, device.serial, cmd);
  }

  return (
    <div className="flex h-[calc(100vh-44px-48px)] flex-col rounded-lg border border-border bg-surface">
      <header className="flex items-center justify-between border-b border-border px-4 py-2">
        <span>
          <b>Android shell</b>{" "}
          <span className="text-muted">(adb shell) — runs as the shell user, not Termux</span>
        </span>
        <span className="flex items-center gap-2 text-muted">
          {deviceTitle(device)}
          <Button variant="ghost" onClick={() => clear(key)} disabled={!!running}>
            Clear
          </Button>
        </span>
      </header>
      <div
        className="flex-1 overflow-auto p-4 font-mono text-[12.5px]"
        role="log"
        aria-label="Terminal output"
      >
        {blocks.map((b) => (
          <div key={b.id} className="mb-3">
            <div className="text-accent">$ {b.command}</div>
            {b.lines.map((l, i) => (
              <div
                key={i}
                className={`whitespace-pre-wrap ${l.stream === "err" ? "text-danger" : ""}`}
                data-stream={l.stream}
              >
                {l.text}
              </div>
            ))}
            {b.error && <ErrorPanel error={b.error} />}
            <div className="mt-1 flex items-center gap-3 text-[11.5px]">
              <Footer b={b} />
              {b.running && b.streamId && (
                <Button variant="ghost" onClick={() => void cancel(key, b.id)}>
                  Cancel
                </Button>
              )}
            </div>
          </div>
        ))}
        <div ref={bottom} />
      </div>
      <form
        onSubmit={onSubmit}
        className="flex items-center gap-2 border-t border-border px-4 py-2 font-mono"
      >
        <span className="text-accent">$</span>
        <input
          aria-label="Command"
          className="flex-1 bg-transparent outline-none"
          placeholder={
            running ? "Waiting for the running command…" : "Type a command and press Enter"
          }
          value={command}
          onChange={(e) => setCommand(e.target.value)}
          autoComplete="off"
          autoFocus
          spellCheck={false}
        />
      </form>
    </div>
  );
}
