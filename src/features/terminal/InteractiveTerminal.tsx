import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef, useState } from "react";
import { Button } from "../../components/Button";
import { ipc, toErrorPayload, type TerminalPtyEvent } from "../../lib/ipc";
import { isModKey, monospaceFont } from "../../lib/platform";

interface InteractiveTerminalProps {
  serial: string;
  initialCommand?: string;
  onClose: () => void;
}

export function InteractiveTerminal({ serial, initialCommand, onClose }: InteractiveTerminalProps) {
  const host = useRef<HTMLDivElement>(null);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const container = host.current;
    if (!container) return;

    let disposed = false;
    let activeSessionId: string | null = null;
    const terminal = new Terminal({
      cursorBlink: true,
      disableStdin: true,
      fontFamily: monospaceFont,
      fontSize: 12,
      scrollback: 1000,
      theme: {
        background: "#0b0d10",
        foreground: "#e6e8eb",
        cursor: "#7aa2f7",
        selectionBackground: "#34415c",
      },
    });
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.open(container);
    fit.fit();
    terminal.attachCustomKeyEventHandler((event) => {
      if (
        event.type === "keydown" &&
        event.key.toLowerCase() === "c" &&
        isModKey(event) &&
        terminal.hasSelection()
      ) {
        event.preventDefault();
        if (!navigator.clipboard) {
          setError("Clipboard is unavailable in this desktop session.");
          return false;
        }
        void navigator.clipboard.writeText(terminal.getSelection()).catch(() => {
          if (!disposed) setError("Could not copy terminal selection.");
        });
        return false;
      }
      return true;
    });

    const output = (event: TerminalPtyEvent) => {
      if (disposed) return;
      if (event.type === "data") terminal.write(Uint8Array.from(event.data));
      else terminal.write("\r\n[session closed]\r\n");
    };

    const dataSubscription = terminal.onData((data) => {
      if (!activeSessionId) return;
      void ipc
        .writeTerminalPty(activeSessionId, Array.from(new TextEncoder().encode(data)))
        .catch((cause: unknown) => {
          if (!disposed) setError(toErrorPayload(cause).message);
        });
    });
    const resizeSubscription = terminal.onResize(({ cols, rows }) => {
      if (!activeSessionId) return;
      void ipc.resizeTerminalPty(activeSessionId, cols, rows).catch((cause: unknown) => {
        if (!disposed) setError(toErrorPayload(cause).message);
      });
    });
    const resizeObserver = new ResizeObserver(() => fit.fit());
    resizeObserver.observe(container);

    void ipc
      .startTerminalPty(serial, terminal.cols, terminal.rows, output)
      .then((id) => {
        activeSessionId = id;
        if (disposed) {
          void ipc.closeTerminalPty(id);
          return;
        }
        setSessionId(id);
        terminal.options.disableStdin = false;
        terminal.focus();
        if (initialCommand) {
          void ipc
            .writeTerminalPty(id, Array.from(new TextEncoder().encode(`${initialCommand}\r`)))
            .catch((cause: unknown) => {
              if (!disposed) setError(toErrorPayload(cause).message);
            });
        }
      })
      .catch((cause: unknown) => {
        if (!disposed) setError(toErrorPayload(cause).message);
      });

    return () => {
      disposed = true;
      resizeObserver.disconnect();
      dataSubscription.dispose();
      resizeSubscription.dispose();
      terminal.dispose();
      if (activeSessionId) void ipc.closeTerminalPty(activeSessionId);
    };
  }, [initialCommand, serial]);

  return (
    <section
      className="flex min-h-0 flex-1 flex-col border-t border-border"
      aria-label="Interactive shell"
    >
      <header className="flex min-h-9 items-center justify-between border-b border-border px-3 text-[11px] text-muted">
        <span>
          {error
            ? "Connection error"
            : sessionId
              ? "Interactive SSH shell"
              : "Connecting to Termux…"}
        </span>
        <Button variant="ghost" onClick={onClose} aria-label="Close interactive shell">
          Close
        </Button>
      </header>
      {error && (
        <p role="alert" className="border-b border-border px-3 py-2 text-danger">
          {error}
        </p>
      )}
      <div
        ref={host}
        className="min-h-0 flex-1 overflow-hidden bg-bg p-2"
        role="application"
        aria-label="Interactive Termux terminal"
      />
    </section>
  );
}
