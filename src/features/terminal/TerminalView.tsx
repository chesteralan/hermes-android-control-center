import { useVirtualizer } from "@tanstack/react-virtual";
import { lazy, Suspense, useCallback, useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { Button } from "../../components/Button";
import { ConfirmDialog } from "../../components/ConfirmDialog";
import { Dialog } from "../../components/Dialog";
import { ErrorPanel } from "../../components/ErrorPanel";
import { ipc } from "../../lib/ipc";
import { AnsiText } from "./AnsiText";
import { deviceKey, useActiveDevice } from "../../stores/devices";
import {
  EMPTY_COMMAND_HISTORY,
  terminalSessionKey,
  terminalHistoryScope,
  useTerminal,
  type CommandBlock,
  type CommandSnippet,
  type OutputLine,
  type TerminalSessionTab,
} from "../../stores/terminal";
import type { ErrorPayload, TransportKind } from "../../types";
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
const NO_TABS: TerminalSessionTab[] = [];
const VIRTUAL_TERMINAL_ROWS = 500;
const InteractiveTerminal = lazy(() =>
  import("./InteractiveTerminal").then((module) => ({ default: module.InteractiveTerminal })),
);

type TerminalRow =
  | { key: string; kind: "command"; block: CommandBlock }
  | { key: string; kind: "line"; blockId: number; line: OutputLine }
  | { key: string; kind: "error"; error: ErrorPayload }
  | { key: string; kind: "footer"; block: CommandBlock };

function flattenTerminalRows(blocks: CommandBlock[]): TerminalRow[] {
  const rows: TerminalRow[] = [];
  for (const block of blocks) {
    rows.push({ key: `command-${block.id}`, kind: "command", block });
    block.lines.forEach((line, index) =>
      rows.push({ key: `line-${block.id}-${index}`, kind: "line", blockId: block.id, line }),
    );
    if (block.error) rows.push({ key: `error-${block.id}`, kind: "error", error: block.error });
    rows.push({ key: `footer-${block.id}`, kind: "footer", block });
  }
  return rows;
}

interface CommandInputState {
  scope: string;
  command: string;
  historyCursor: number | null;
  draftCommand: string;
  reverseSearchActive: boolean;
  reverseSearchCursor: number | null;
}

export const TRANSPORT_LABEL: Record<TransportKind, string> = {
  adbShell: "Android shell",
  termuxSsh: "Termux",
  api: "Hermes API",
};

function TerminalOutputRow({
  row,
  onCancel,
  onCopyText,
}: {
  row: TerminalRow;
  onCancel: (blockId: number) => void;
  onCopyText: (text: string) => void;
}) {
  if (row.kind === "command") {
    return (
      <div className="mb-1 text-accent">
        <span className="mr-2 rounded bg-surface-2 px-1.5 text-[10.5px] text-muted">
          {TRANSPORT_LABEL[row.block.transport]}
        </span>
        $ {row.block.command}
      </div>
    );
  }
  if (row.kind === "line") {
    return (
      <div
        className={`whitespace-pre-wrap ${row.line.stream === "err" ? "text-danger" : ""}`}
        data-stream={row.line.stream}
      >
        <AnsiText text={row.line.text} />
      </div>
    );
  }
  if (row.kind === "error") return <ErrorPanel error={row.error} />;
  return (
    <div className="mb-3 mt-1 flex items-center gap-3 text-[11.5px]">
      <Footer b={row.block} />
      <Button
        variant="ghost"
        onClick={() => onCopyText(row.block.lines.map((line) => line.text).join("\n"))}
        disabled={row.block.lines.length === 0}
      >
        Copy block
      </Button>
      {row.block.running && row.block.streamId && (
        <Button variant="ghost" onClick={() => onCancel(row.block.id)}>
          Cancel
        </Button>
      )}
    </div>
  );
}

export function TerminalView() {
  const device = useActiveDevice();
  const key = device ? deviceKey(device) : "";
  const tabs = useTerminal((state) => state.tabsByDevice[key] ?? NO_TABS);
  const activeSessionId = useTerminal((state) => state.activeSessionByDevice[key] ?? null);
  const sessionKey = activeSessionId ? terminalSessionKey(key, activeSessionId) : "";
  const blocks = useTerminal((state) => state.sessions[sessionKey]?.blocks ?? NO_BLOCKS);
  const { run, cancel, clear } = useTerminal();
  const createSession = useTerminal((state) => state.createSession);
  const ensureSession = useTerminal((state) => state.ensureSession);
  const selectSession = useTerminal((state) => state.selectSession);
  const closeSession = useTerminal((state) => state.closeSession);
  const snippets = useTerminal((state) => state.snippets);
  const addSnippet = useTerminal((state) => state.addSnippet);
  const removeSnippet = useTerminal((state) => state.removeSnippet);
  const [transport, setTransport] = useState<TransportKind>("adbShell");
  const [interactive, setInteractive] = useState(false);
  const [commandTimeoutSeconds, setCommandTimeoutSeconds] = useState(0);
  const [snippetDialogOpen, setSnippetDialogOpen] = useState(false);
  const [snippetName, setSnippetName] = useState("");
  const [snippetCommand, setSnippetCommand] = useState("");
  const [snippetTransport, setSnippetTransport] = useState<TransportKind>("adbShell");
  const [snippetRequiresConfirmation, setSnippetRequiresConfirmation] = useState(true);
  const [pendingSnippet, setPendingSnippet] = useState<CommandSnippet | null>(null);
  const [pendingCloseSessionId, setPendingCloseSessionId] = useState<string | null>(null);
  const snippetForm = useRef<HTMLFormElement>(null);
  const closeSnippetDialog = useCallback(() => setSnippetDialogOpen(false), []);
  const [actionStatus, setActionStatus] = useState("");
  const rows = flattenTerminalRows(blocks);
  const scroller = useRef<HTMLDivElement>(null);
  // eslint-disable-next-line react-hooks/incompatible-library -- virtualizer state is read during render by design
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => 22,
    initialRect: { width: 800, height: 600 },
    overscan: 20,
  });
  const historyScope = device ? terminalHistoryScope(key, transport) : "";
  const history = useTerminal((state) => state.historyByScope[historyScope] ?? EMPTY_COMMAND_HISTORY);
  const loadCommandHistory = useTerminal((state) => state.loadCommandHistory);
  const historyPersistenceEnabled = useTerminal((state) => state.historyPersistenceEnabled);
  const setHistoryPersistence = useTerminal((state) => state.setHistoryPersistence);
  const [inputState, setInputState] = useState<CommandInputState>({
    scope: historyScope,
    command: "",
    historyCursor: null,
    draftCommand: "",
    reverseSearchActive: false,
    reverseSearchCursor: null,
  });
  const activeInputState =
    inputState.scope === `${historyScope}:${activeSessionId ?? ""}`
      ? inputState
      : {
          scope: `${historyScope}:${activeSessionId ?? ""}`,
          command: "",
          historyCursor: null,
          draftCommand: "",
          reverseSearchActive: false,
          reverseSearchCursor: null,
        };
  const { command, historyCursor, draftCommand, reverseSearchActive, reverseSearchCursor } =
    activeInputState;
  function updateInputState(update: Partial<Omit<CommandInputState, "scope">>) {
    setInputState((current) => ({
      ...(current.scope === historyScope ? current : activeInputState),
      ...update,
      scope: `${historyScope}:${activeSessionId ?? ""}`,
    }));
  }
  const bottom = useRef<HTMLDivElement>(null);
  const running = blocks.find((b) => b.running);
  const lineCount = blocks.reduce((n, b) => n + b.lines.length, 0);
  const virtualize = lineCount > VIRTUAL_TERMINAL_ROWS;
  let reverseMatchIndex: number | null = null;
  if (reverseSearchActive) {
    const query = command.trim().toLowerCase();
    const upperBound = Math.min(reverseSearchCursor ?? history.length, history.length);
    for (let index = upperBound - 1; index >= 0; index -= 1) {
      if (history[index]?.toLowerCase().includes(query)) {
        reverseMatchIndex = index;
        break;
      }
    }
  }

  useEffect(() => {
    if (historyScope) loadCommandHistory(historyScope);
  }, [historyScope, loadCommandHistory]);

  useEffect(() => {
    if (device && !activeSessionId) ensureSession(key);
  }, [activeSessionId, device, ensureSession, key]);

  const requestCloseSession = useCallback((sessionId: string) => {
    const session = useTerminal.getState().sessions[terminalSessionKey(key, sessionId)];
    if (session?.blocks.some((block) => block.running)) {
      setPendingCloseSessionId(sessionId);
    } else {
      void closeSession(key, sessionId);
    }
  }, [closeSession, key]);

  useEffect(() => {
    if (!device) return;
    const onShortcut = (event: globalThis.KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey) return;
      if (event.key.toLowerCase() === "t") {
        event.preventDefault();
        createSession(key);
      } else if (event.key.toLowerCase() === "w" && activeSessionId && tabs.length > 1) {
        event.preventDefault();
        requestCloseSession(activeSessionId);
      }
    };
    window.addEventListener("keydown", onShortcut);
    return () => window.removeEventListener("keydown", onShortcut);
  }, [activeSessionId, createSession, device, key, requestCloseSession, tabs.length]);

  useEffect(() => {
    if (virtualize) virtualizer.scrollToIndex(rows.length - 1, { align: "end" });
    else bottom.current?.scrollIntoView?.({ block: "end" });
  }, [blocks.length, lineCount, rows.length, virtualize, virtualizer]);

  if (!device) return <NoDeviceState />;

  function onSubmit(e: FormEvent) {
    e.preventDefault();
    if (reverseSearchActive) {
      updateInputState({
        command: reverseMatchIndex === null ? command : (history[reverseMatchIndex] ?? ""),
        historyCursor: reverseMatchIndex,
        reverseSearchActive: false,
        reverseSearchCursor: null,
      });
      return;
    }
    const cmd = command.trim();
    if (!cmd || running || !device || !activeSessionId) return;
    updateInputState({ command: "", historyCursor: null, draftCommand: "" });
    void run(key, device.serial, cmd, transport, activeSessionId, commandTimeoutSeconds * 1000);
  }

  function onCommandKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.ctrlKey && event.key.toLowerCase() === "c" && running && activeSessionId) {
      event.preventDefault();
      void cancel(key, activeSessionId, running.id);
      return;
    }

    if (event.ctrlKey && event.key.toLowerCase() === "r") {
      event.preventDefault();
      if (!history.length) return;
      updateInputState({
        reverseSearchCursor: reverseSearchActive
          ? (reverseMatchIndex ?? reverseSearchCursor ?? history.length)
          : history.length,
        reverseSearchActive: true,
      });
      return;
    }

    if (reverseSearchActive) {
      if (event.key === "Escape") {
        event.preventDefault();
        updateInputState({ reverseSearchActive: false, reverseSearchCursor: null });
      }
      return;
    }

    if (event.key === "ArrowUp") {
      event.preventDefault();
      if (!history.length) return;
      if (historyCursor === null) updateInputState({ draftCommand: command });
      const nextIndex = historyCursor === null ? history.length - 1 : Math.max(0, historyCursor - 1);
      updateInputState({ historyCursor: nextIndex, command: history[nextIndex] ?? "" });
    } else if (event.key === "ArrowDown" && historyCursor !== null) {
      event.preventDefault();
      if (historyCursor >= history.length - 1) {
        updateInputState({ historyCursor: null, command: draftCommand });
      } else {
        const nextIndex = historyCursor + 1;
        updateInputState({ historyCursor: nextIndex, command: history[nextIndex] ?? "" });
      }
    }
  }

  function onCommandChange(value: string) {
    if (reverseSearchActive) {
      updateInputState({ command: value, reverseSearchCursor: history.length });
    } else {
      updateInputState({ command: value, historyCursor: null });
    }
  }

  function runSnippet(snippet: CommandSnippet) {
    if (!device || running) return;
    if (snippet.requiresConfirmation) {
      setPendingSnippet(snippet);
      return;
    }
    if (!activeSessionId) return;
    void run(
      key,
      device.serial,
      snippet.command,
      snippet.transport,
      activeSessionId,
      commandTimeoutSeconds * 1000,
    );
  }

  function saveSnippet(event: FormEvent) {
    event.preventDefault();
    if (!snippetName.trim() || !snippetCommand.trim()) return;
    addSnippet({
      name: snippetName,
      command: snippetCommand,
      transport: snippetTransport,
      requiresConfirmation: snippetRequiresConfirmation,
    });
    setSnippetName("");
    setSnippetCommand("");
    setSnippetTransport(transport);
    setSnippetRequiresConfirmation(true);
    setSnippetDialogOpen(false);
  }

  async function copyText(text: string) {
    try {
      if (!navigator.clipboard) throw new Error("Clipboard access is unavailable.");
      await navigator.clipboard.writeText(text);
      setActionStatus("Copied");
    } catch {
      setActionStatus("Clipboard unavailable");
    }
  }

  async function exportSession() {
    const text = blocks
      .map((block) =>
        [`$ ${block.command}`, ...block.lines.map((line) => line.text)].join("\n"),
      )
      .join("\n\n");
    try {
      const path = await ipc.exportTerminalText(text);
      setActionStatus(path ? "Terminal output exported" : "Export canceled");
    } catch {
      setActionStatus("Terminal export failed");
    }
  }

  return (
    <div className="flex h-[calc(100vh-44px-48px)] flex-col rounded-lg border border-border bg-surface">
      <header className="flex items-center justify-between border-b border-border px-4 py-2">
        <span className="flex items-center gap-3">
          <span
            role="radiogroup"
            aria-label="Run commands in"
            className="flex rounded-md bg-bg p-0.5"
          >
            {(Object.keys(TRANSPORT_LABEL) as TransportKind[])
              .filter((t) => t !== "api")
              .map((t) => (
                <button
                  key={t}
                  type="button"
                  role="radio"
                  aria-checked={transport === t}
                  onClick={() => {
                    setTransport(t);
                    if (t !== "termuxSsh") setInteractive(false);
                  }}
                  className={`rounded px-2.5 py-1 ${transport === t ? "bg-surface-2 text-text" : "text-muted"}`}
                >
                  {TRANSPORT_LABEL[t]}
                </button>
              ))}
          </span>
          <span className="text-muted">
            {transport === "adbShell"
              ? "(adb shell) — runs as the shell user, not Termux"
              : "(SSH) — runs as the Termux user"}
          </span>
        </span>
        <span className="flex items-center gap-2 text-muted">
          {deviceTitle(device)}
          {transport === "termuxSsh" && (
            <Button
              variant="ghost"
              aria-pressed={interactive}
              onClick={() => setInteractive((active) => !active)}
            >
              {interactive ? "Close interactive shell" : "Interactive shell"}
            </Button>
          )}
          <label className="flex items-center gap-1.5 text-[11px]">
            <input
              type="checkbox"
              aria-label="Save command history"
              checked={historyPersistenceEnabled}
              onChange={(event) => setHistoryPersistence(event.target.checked)}
            />
            Save history
          </label>
          <label className="flex items-center gap-1.5 text-[11px]" title="0 disables the timeout">
            Timeout
            <input
              aria-label="Command timeout in seconds"
              className="w-14 rounded border border-border bg-bg px-1 py-0.5 text-text"
              type="number"
              min={0}
              max={3600}
              step={1}
              value={commandTimeoutSeconds}
              onChange={(event) => {
                const seconds = Number(event.target.value);
                setCommandTimeoutSeconds(
                  Number.isFinite(seconds) ? Math.min(3600, Math.max(0, seconds)) : 0,
                );
              }}
            />
            s
          </label>
          <Button
            variant="ghost"
            onClick={() => {
              setSnippetTransport(transport);
              setSnippetCommand(command);
              setSnippetDialogOpen(true);
            }}
            disabled={!!running || !command.trim()}
          >
            Save snippet
          </Button>
          <Button variant="ghost" onClick={() => activeSessionId && clear(key, activeSessionId)} disabled={!!running}>
            Clear
          </Button>
          <Button variant="ghost" onClick={() => void exportSession()} disabled={blocks.length === 0}>
            Export .txt
          </Button>
          <Button
            variant="ghost"
            onClick={() =>
              void copyText(
                blocks
                  .map((block) =>
                    [`$ ${block.command}`, ...block.lines.map((line) => line.text)].join("\n"),
                  )
                  .join("\n\n"),
              )
            }
            disabled={blocks.length === 0}
          >
            Copy all
          </Button>
          {actionStatus && <span role="status" className="text-[10px]">{actionStatus}</span>}
        </span>
      </header>
      <nav
        role="tablist"
        aria-label="Terminal sessions"
        className="flex min-h-10 items-center gap-1 overflow-x-auto border-b border-border px-2"
      >
        {tabs.map((tab) => (
          <span key={tab.id} className="inline-flex shrink-0 items-center">
            <button
              type="button"
              role="tab"
              aria-selected={activeSessionId === tab.id}
              onClick={() => selectSession(key, tab.id)}
              className={`rounded px-2.5 py-1 text-[12px] ${activeSessionId === tab.id ? "bg-surface-2 text-text" : "text-muted hover:bg-surface-2"}`}
            >
              {tab.title}
            </button>
            <button
              type="button"
              aria-label={`Close ${tab.title}`}
              disabled={tabs.length < 2}
              onClick={() => requestCloseSession(tab.id)}
              className="px-1 text-[11px] text-muted hover:text-danger disabled:opacity-40"
            >
              ×
            </button>
          </span>
        ))}
        <Button variant="ghost" aria-label="New terminal session" onClick={() => createSession(key)}>
          New session
        </Button>
      </nav>
      {snippets.length > 0 && (
        <aside
          aria-label="Saved command snippets"
          className="flex flex-wrap items-center gap-2 border-b border-border px-3 py-2"
        >
          <span className="text-[10px] font-semibold uppercase text-muted">Snippets</span>
          {snippets.map((snippet) => (
            <span key={snippet.id} className="inline-flex items-center gap-1">
              <button
                type="button"
                onClick={() => runSnippet(snippet)}
                disabled={!!running}
                title={`${TRANSPORT_LABEL[snippet.transport]}${snippet.requiresConfirmation ? " · confirmation required" : ""}`}
                className="rounded border border-border bg-bg px-2 py-1 text-[11px] text-text hover:border-accent disabled:opacity-50"
              >
                {snippet.name}
              </button>
              <button
                type="button"
                aria-label={`Remove ${snippet.name}`}
                onClick={() => removeSnippet(snippet.id)}
                disabled={!!running}
                className="px-1 text-[11px] text-muted hover:text-danger disabled:opacity-50"
              >
                ×
              </button>
            </span>
          ))}
        </aside>
      )}
      {interactive ? (
        <Suspense fallback={<p className="p-3 text-muted">Loading interactive terminal…</p>}>
          <InteractiveTerminal
            key={`${key}:${activeSessionId ?? ""}`}
            serial={device.serial}
            onClose={() => setInteractive(false)}
          />
        </Suspense>
      ) : (
        <>
      <div
        ref={scroller}
        className="flex-1 overflow-auto p-4 font-mono text-[12.5px]"
        role="log"
        aria-label="Terminal output"
      >
        {virtualize ? (
          <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
            {virtualizer.getVirtualItems().map((item) => {
              const row = rows[item.index];
              return row ? (
                <div
                  key={row.key}
                  data-index={item.index}
                  ref={virtualizer.measureElement}
                  className="absolute left-0 right-0 top-0"
                  style={{ transform: `translateY(${item.start}px)` }}
                >
                  <TerminalOutputRow
                    row={row}
                    onCancel={(blockId) => activeSessionId && void cancel(key, activeSessionId, blockId)}
                    onCopyText={(text) => void copyText(text)}
                  />
                </div>
              ) : null;
            })}
          </div>
        ) : (
          <>
            {rows.map((row) => (
              <TerminalOutputRow
                key={row.key}
                row={row}
                onCancel={(blockId) => activeSessionId && void cancel(key, activeSessionId, blockId)}
                onCopyText={(text) => void copyText(text)}
              />
            ))}
            <div ref={bottom} />
          </>
        )}
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
          onChange={(e) => onCommandChange(e.target.value)}
          onKeyDown={onCommandKeyDown}
          autoComplete="off"
          autoFocus
          spellCheck={false}
        />
      </form>
      {reverseSearchActive && (
        <p className="border-t border-border px-4 py-1 text-[11px] text-muted" role="status">
          {reverseMatchIndex === null
            ? "No history match"
            : `Reverse search: ${history[reverseMatchIndex]}`}
        </p>
      )}
        </>
      )}
      <Dialog
        open={snippetDialogOpen}
        title="Save command snippet"
        onClose={closeSnippetDialog}
        footer={
          <>
            <Button onClick={closeSnippetDialog}>Cancel</Button>
            <Button
              variant="primary"
              onClick={() => snippetForm.current?.requestSubmit()}
              disabled={!snippetName.trim() || !snippetCommand.trim()}
            >
              Save snippet
            </Button>
          </>
        }
      >
        <form ref={snippetForm} className="space-y-3" onSubmit={saveSnippet}>
          <label className="block text-[12px] text-muted" htmlFor="snippet-name">
            Name
          </label>
          <input
            id="snippet-name"
            value={snippetName}
            onChange={(event) => setSnippetName(event.target.value)}
            className="w-full rounded border border-border bg-bg px-2 py-1.5 text-[13px] text-text"
            maxLength={80}
          />
          <label className="block text-[12px] text-muted" htmlFor="snippet-command">
            Snippet command
          </label>
          <textarea
            id="snippet-command"
            value={snippetCommand}
            onChange={(event) => setSnippetCommand(event.target.value)}
            className="w-full rounded border border-border bg-bg px-2 py-1.5 font-mono text-[12px] text-text"
            rows={3}
          />
          <label className="block text-[12px] text-muted" htmlFor="snippet-transport">
            Transport
          </label>
          <select
            id="snippet-transport"
            value={snippetTransport}
            onChange={(event) => setSnippetTransport(event.target.value as TransportKind)}
            className="w-full rounded border border-border bg-bg px-2 py-1.5 text-[12px] text-text"
          >
            {(Object.keys(TRANSPORT_LABEL) as TransportKind[]).map((kind) => (
              <option key={kind} value={kind}>
                {TRANSPORT_LABEL[kind]}
              </option>
            ))}
          </select>
          <label className="flex items-center gap-2 text-[12px] text-text">
            <input
              type="checkbox"
              checked={snippetRequiresConfirmation}
              onChange={(event) => setSnippetRequiresConfirmation(event.target.checked)}
            />
            Require confirmation before running
          </label>
        </form>
      </Dialog>
      <ConfirmDialog
        open={pendingCloseSessionId !== null}
        title="Close terminal session?"
        confirmLabel="Close session"
        onCancel={() => setPendingCloseSessionId(null)}
        onConfirm={() => {
          if (pendingCloseSessionId) void closeSession(key, pendingCloseSessionId);
          setPendingCloseSessionId(null);
        }}
      >
        Closing this tab cancels its running command.
      </ConfirmDialog>
      <ConfirmDialog
        open={pendingSnippet !== null}
        title="Run saved command?"
        confirmLabel="Run command"
        onCancel={() => setPendingSnippet(null)}
        onConfirm={() => {
          if (pendingSnippet && device) {
            void run(
              key,
              device.serial,
              pendingSnippet.command,
              pendingSnippet.transport,
              activeSessionId ?? undefined,
              commandTimeoutSeconds * 1000,
            );
          }
          setPendingSnippet(null);
        }}
      >
        <p className="mb-2 text-[12px] text-muted">
          {pendingSnippet?.name} · {pendingSnippet ? TRANSPORT_LABEL[pendingSnippet.transport] : ""}
        </p>
        <pre className="whitespace-pre-wrap break-all rounded border border-border bg-bg p-3 font-mono text-[12px] text-text">
          {pendingSnippet?.command}
        </pre>
      </ConfirmDialog>
    </div>
  );
}
