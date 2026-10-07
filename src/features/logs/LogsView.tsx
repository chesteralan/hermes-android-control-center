import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from "react";
import { Button } from "../../components/Button";
import { Dialog } from "../../components/Dialog";
import { ErrorPanel } from "../../components/ErrorPanel";
import { ipc, type LogExportFormat } from "../../lib/ipc";
import { deviceKey, useActiveDevice } from "../../stores/devices";
import { useLogs } from "../../stores/logs";
import type { LogLevel, LogLine, LogSourceKind } from "../../types";
import type { LogSearchWorkerRequest, LogSearchWorkerResponse } from "./logSearch.worker";
import { NoDeviceState } from "../device/NoDeviceState";

const ROW = 20;
const WORKER_SEARCH_LINE_THRESHOLD = 10_000;
const NO_LINES: LogLine[] = [];
const LEVEL_FILTERS = ["info", "warn", "error", "debug", "unknown"] as const;
type LevelFilter = (typeof LEVEL_FILTERS)[number];
const LOG_SOURCES: Array<{ value: LogSourceKind; label: string }> = [
  { value: "logcat", label: "Android logcat" },
  { value: "hermesGateway", label: "Hermes gateway" },
  { value: "hermesToolCalls", label: "Hermes tool calls" },
  { value: "supervisor", label: "Supervisor" },
];

interface PauseSnapshot {
  deviceKey: string;
  lines: LogLine[];
  lastSequence: number;
}

const levelStyle: Record<LogLevel, string> = {
  debug: "text-muted",
  info: "text-accent",
  warn: "text-warning",
  error: "text-danger",
};

export function LevelBadge({ level }: { level: LogLevel | null }) {
  return (
    <span
      className={`inline-block w-12 shrink-0 uppercase ${level ? levelStyle[level] : "text-muted"}`}
    >
      {level ?? ""}
    </span>
  );
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function HighlightText({ text, pattern }: { text: string; pattern: RegExp | null }): ReactNode {
  if (!pattern) return text;
  const flags = pattern.flags.includes("g") ? pattern.flags : `${pattern.flags}g`;
  const matches = [...text.matchAll(new RegExp(pattern.source, flags))];
  if (matches.length === 0) return text;
  const nodes: ReactNode[] = [];
  let cursor = 0;
  for (const [index, match] of matches.entries()) {
    const start = match.index ?? cursor;
    if (start > cursor) nodes.push(text.slice(cursor, start));
    if (match[0].length > 0) {
      nodes.push(
        <mark key={`match-${index}`} className="rounded bg-warning/30 text-text">
          {match[0]}
        </mark>,
      );
      cursor = start + match[0].length;
    }
  }
  if (cursor < text.length) nodes.push(text.slice(cursor));
  return nodes;
}

export function LogRow({
  line,
  pattern = null,
  activeMatch = false,
  selected = false,
  onSelect,
}: {
  line: LogLine;
  pattern?: RegExp | null;
  activeMatch?: boolean;
  selected?: boolean;
  onSelect?: (event: MouseEvent<HTMLButtonElement>) => void;
}) {
  return (
    <button
      type="button"
      aria-label={`Select log line ${line.seq}`}
      aria-pressed={selected}
      data-seq={line.seq}
      onClick={onSelect}
      className={`flex w-full gap-3 whitespace-pre px-3 text-left font-mono text-[12px] ${activeMatch ? "bg-accent/10" : ""} ${selected ? "ring-1 ring-accent" : ""}`}
      style={{ height: ROW, lineHeight: `${ROW}px` }}
    >
      <span className="w-[140px] shrink-0 text-muted">{line.timestamp ?? ""}</span>
      <LevelBadge level={line.level} />
      <span className="truncate">
        {line.tag && <span className="text-muted"><HighlightText text={`${line.tag}: `} pattern={pattern} /></span>}
        <HighlightText text={line.message} pattern={pattern} />
      </span>
    </button>
  );
}

export function LogsView() {
  const device = useActiveDevice();
  const key = device ? deviceKey(device) : "";
  const entry = useLogs((s) => s.byDevice[key]);
  const { start, stop, clear } = useLogs();
  const lines = entry?.lines ?? NO_LINES;
  const streaming = !!entry?.streamId;
  const [follow, setFollow] = useState(true);
  const [pauseSnapshot, setPauseSnapshot] = useState<PauseSnapshot | null>(null);
  const [searchInput, setSearchInput] = useState("");
  const [searchQuery, setSearchQuery] = useState("");
  const [regexEnabled, setRegexEnabled] = useState(false);
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [matchCursor, setMatchCursor] = useState(0);
  const [exportDialogOpen, setExportDialogOpen] = useState(false);
  const [exportScope, setExportScope] = useState<"visible" | "all">("visible");
  const [exportFormat, setExportFormat] = useState<LogExportFormat>("log");
  const [exportStatus, setExportStatus] = useState("");
  const [source, setSource] = useState<LogSourceKind>("logcat");
  const [selectedLevels, setSelectedLevels] = useState<Set<LevelFilter>>(
    () => new Set(LEVEL_FILTERS),
  );
  const [workerResult, setWorkerResult] = useState<LogSearchWorkerResponse | null>(null);
  const workerRef = useRef<Worker | null>(null);
  const workerRequestId = useRef(0);
  const [selectionState, setSelectionState] = useState<{
    deviceKey: string;
    selectedSeqs: Set<number>;
    anchorSeq: number | null;
  }>({ deviceKey: key, selectedSeqs: new Set(), anchorSeq: null });
  const selectedSeqs =
    selectionState.deviceKey === key ? selectionState.selectedSeqs : new Set<number>();
  const paused = pauseSnapshot?.deviceKey === key;
  const visibleLines = paused && pauseSnapshot ? pauseSnapshot.lines : lines;
  const lastSequence = lines.at(-1)?.seq ?? -1;
  const displayedSource = LOG_SOURCES.find((option) => option.value === (entry?.source ?? source));
  const newLineCount = paused && pauseSnapshot ? Math.max(0, lastSequence - pauseSnapshot.lastSequence) : 0;
  const scroller = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const timeout = window.setTimeout(() => setSearchQuery(searchInput), 180);
    return () => window.clearTimeout(timeout);
  }, [searchInput]);

  let searchPattern: RegExp | null = null;
  let searchError: string | null = null;
  if (searchQuery) {
    try {
      searchPattern = new RegExp(
        regexEnabled ? searchQuery : escapeRegExp(searchQuery),
        caseSensitive ? "" : "i",
      );
    } catch {
      searchError = "Invalid regular expression.";
    }
  }

  const useSearchWorker =
    visibleLines.length > WORKER_SEARCH_LINE_THRESHOLD && searchQuery.length > 0 && !searchError;
  const workerSearchKey = JSON.stringify([
    key,
    entry?.source ?? source,
    paused,
    searchQuery,
    regexEnabled,
    caseSensitive,
    visibleLines.length,
    visibleLines[0]?.seq ?? null,
    visibleLines.at(-1)?.seq ?? null,
  ]);
  const currentWorkerResult =
    useSearchWorker && workerResult?.signature === workerSearchKey ? workerResult : null;
  const workerMatchSet = currentWorkerResult ? new Set(currentWorkerResult.matchedSeqs) : null;
  const searchPending = useSearchWorker && workerMatchSet === null;
  const searchMatches = useSearchWorker
    ? workerMatchSet
      ? visibleLines.filter((line) => workerMatchSet.has(line.seq))
      : []
    : searchPattern
      ? visibleLines.filter((line) => searchPattern?.test(`${line.timestamp ?? ""} ${line.level ?? ""} ${line.tag ?? ""} ${line.message}`))
      : visibleLines;
  const levelCounts = searchMatches.reduce<Record<LevelFilter, number>>(
    (counts, line) => {
      counts[line.level ?? "unknown"] += 1;
      return counts;
    },
    { info: 0, warn: 0, error: 0, debug: 0, unknown: 0 },
  );
  const matchingLines = searchMatches.filter((line) => selectedLevels.has(line.level ?? "unknown"));
  const activeMatchIndex = matchingLines.length === 0 ? -1 : Math.min(matchCursor, matchingLines.length - 1);
  const activeMatchSequence = searchPattern && activeMatchIndex >= 0 ? matchingLines[activeMatchIndex]?.seq : null;
  const renderedLines = matchingLines;

  useEffect(() => {
    if (typeof Worker === "undefined") return;
    const worker = new Worker(new URL("./logSearch.worker.ts", import.meta.url), { type: "module" });
    workerRef.current = worker;
    worker.onmessage = (event: MessageEvent<LogSearchWorkerResponse>) => setWorkerResult(event.data);
    return () => {
      worker.terminate();
      workerRef.current = null;
    };
  }, []);

  useEffect(() => {
    if (!useSearchWorker || !workerRef.current) return;
    const requestId = workerRequestId.current + 1;
    workerRequestId.current = requestId;
    const request: LogSearchWorkerRequest = {
      requestId,
      signature: workerSearchKey,
      query: searchQuery,
      regex: regexEnabled,
      caseSensitive,
      lines: visibleLines.map((line) => ({
        seq: line.seq,
        text: `${line.timestamp ?? ""} ${line.level ?? ""} ${line.tag ?? ""} ${line.message}`,
      })),
    };
    const timeout = window.setTimeout(() => workerRef.current?.postMessage(request), 100);
    return () => window.clearTimeout(timeout);
  }, [caseSensitive, regexEnabled, searchQuery, useSearchWorker, visibleLines, workerSearchKey]);

  // eslint-disable-next-line react-hooks/incompatible-library -- virtualizer state is read during render by design
  const virt = useVirtualizer({
    count: renderedLines.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW,
    overscan: 20,
  });

  useEffect(() => {
    if (!paused && follow && renderedLines.length > 0) {
      virt.scrollToIndex(renderedLines.length - 1, { align: "end" });
    }
  }, [follow, paused, renderedLines.length, virt]);

  if (!device) return <NoDeviceState />;

  const onScroll = () => {
    if (paused) return;
    const el = scroller.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < ROW * 2;
    if (atBottom !== follow) setFollow(atBottom);
  };

  function selectLine(event: MouseEvent<HTMLButtonElement>, line: LogLine, index: number) {
    if (event.shiftKey && selectionState.deviceKey === key && selectionState.anchorSeq !== null) {
      const anchorIndex = renderedLines.findIndex((item) => item.seq === selectionState.anchorSeq);
      if (anchorIndex >= 0) {
        const start = Math.min(anchorIndex, index);
        const end = Math.max(anchorIndex, index);
        setSelectionState({
          deviceKey: key,
          selectedSeqs: new Set(renderedLines.slice(start, end + 1).map((item) => item.seq)),
          anchorSeq: selectionState.anchorSeq,
        });
        return;
      }
    }

    const toggle = event.metaKey || event.ctrlKey;
    const next = toggle && selectionState.deviceKey === key
      ? new Set(selectionState.selectedSeqs)
      : new Set<number>();
    if (toggle && next.has(line.seq)) next.delete(line.seq);
    else next.add(line.seq);
    setSelectionState({
      deviceKey: key,
      selectedSeqs: next,
      anchorSeq: toggle && selectionState.deviceKey === key ? selectionState.anchorSeq : line.seq,
    });
  }

  async function copyRawLines(selectedOnly: boolean) {
    const copyLines = selectedOnly
      ? renderedLines.filter((line) => selectedSeqs.has(line.seq))
      : renderedLines;
    try {
      await navigator.clipboard.writeText(copyLines.map((line) => line.raw).join("\n"));
    } catch {
      return;
    }
  }

  async function exportLogs() {
    const exportLines = exportScope === "visible" ? renderedLines : lines;
    setExportDialogOpen(false);
    try {
      const path = await ipc.exportLogs(exportLines, exportFormat);
      setExportStatus(path ? "Logs exported" : "Export canceled");
    } catch {
      setExportStatus("Log export failed");
    }
  }

  function onLogKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "c" && selectedSeqs.size) {
      event.preventDefault();
      void copyRawLines(true);
    }
  }

  return (
    <div className="flex h-[calc(100vh-44px-48px)] min-w-0 flex-col rounded-lg border border-border bg-surface">
      <header className="flex shrink-0 flex-col gap-2 border-b border-border px-4 py-2">
        <span>
          <b>Logs</b>{" "}
          <span className="text-muted">{displayedSource?.label} · {lines.length.toLocaleString()} lines</span>
        </span>
        <span className="flex min-w-0 flex-wrap items-center gap-2">
          <select
            aria-label="Log source"
            value={source}
            disabled={streaming || !!entry?.starting}
            onChange={(event) => setSource(event.target.value as LogSourceKind)}
            className="rounded border border-border bg-bg px-2 py-1 text-[12px] text-text"
          >
            {LOG_SOURCES.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
          {streaming ? (
            <Button onClick={() => void stop(key)}>Stop</Button>
          ) : (
            <Button
              variant="primary"
              loading={entry?.starting}
              onClick={() => void start(key, device.serial, source)}
            >
              Start
            </Button>
          )}
          <Button variant="ghost" onClick={() => clear(key)}>
            Clear
          </Button>
          <Button
            variant="ghost"
            onClick={() => void copyRawLines(false)}
            disabled={renderedLines.length === 0}
          >
            Copy visible
          </Button>
          <Button
            variant="ghost"
            onClick={() => setExportDialogOpen(true)}
            disabled={lines.length === 0}
          >
            Export
          </Button>
          <label className="flex items-center gap-1 text-[11px] text-muted">
            Search
            <input
              type="search"
              aria-label="Search logs"
              value={searchInput}
              onChange={(event) => {
                setSearchInput(event.target.value);
                setMatchCursor(0);
              }}
              className="w-32 rounded border border-border bg-bg px-2 py-1 text-text"
            />
          </label>
          <label className="flex items-center gap-1 text-[11px] text-muted">
            <input
              type="checkbox"
              aria-label="Use regular expression"
              checked={regexEnabled}
              onChange={(event) => {
                setRegexEnabled(event.target.checked);
                setMatchCursor(0);
              }}
            />
            Regex
          </label>
          <label className="flex items-center gap-1 text-[11px] text-muted">
            <input
              type="checkbox"
              aria-label="Case sensitive"
              checked={caseSensitive}
              onChange={(event) => {
                setCaseSensitive(event.target.checked);
                setMatchCursor(0);
              }}
            />
            Case
          </label>
          <span className="flex max-w-full flex-wrap items-center gap-1" role="group" aria-label="Filter log levels">
            <Button
              variant="ghost"
              aria-pressed={selectedLevels.size === LEVEL_FILTERS.length}
              onClick={() => setSelectedLevels(new Set(LEVEL_FILTERS))}
            >
              All
            </Button>
            {LEVEL_FILTERS.map((level) => (
              <Button
                key={level}
                variant="ghost"
                aria-label={`${level.toUpperCase()} ${levelCounts[level]}`}
                aria-pressed={selectedLevels.has(level)}
                onClick={() =>
                  setSelectedLevels((current) => {
                    const next = new Set(current);
                    if (next.has(level)) next.delete(level);
                    else next.add(level);
                    return next;
                  })
                }
              >
                {level === "unknown" ? "Unknown" : level.toUpperCase()} {levelCounts[level]}
              </Button>
            ))}
          </span>
          <span role={searchError ? "alert" : "status"} className="text-[10px] text-muted">
            {searchError ?? (searchPending ? "Searching…" : searchQuery ? `${renderedLines.length} matches` : "")}
          </span>
          <Button
            variant="ghost"
            aria-label="Previous log match"
            disabled={renderedLines.length === 0 || searchPending}
            onClick={() => {
              const next = (activeMatchIndex - 1 + renderedLines.length) % renderedLines.length;
              setMatchCursor(next);
              const sequence = renderedLines[next]?.seq;
              const index = renderedLines.findIndex((line) => line.seq === sequence);
              if (index >= 0) virt.scrollToIndex(index, { align: "center" });
            }}
          >
            Previous
          </Button>
          <Button
            variant="ghost"
            aria-label="Next log match"
            disabled={renderedLines.length === 0 || searchPending}
            onClick={() => {
              const next = (activeMatchIndex + 1) % renderedLines.length;
              setMatchCursor(next);
              const sequence = renderedLines[next]?.seq;
              const index = renderedLines.findIndex((line) => line.seq === sequence);
              if (index >= 0) virt.scrollToIndex(index, { align: "center" });
            }}
          >
            Next
          </Button>
          {paused ? (
            <Button
              variant="ghost"
              onClick={() => {
                setPauseSnapshot(null);
                setFollow(true);
              }}
            >
              Resume{newLineCount > 0 ? ` · ${newLineCount} new` : ""}
            </Button>
          ) : (
            <Button
              variant="ghost"
              disabled={lines.length === 0}
              onClick={() => {
                setPauseSnapshot({ deviceKey: key, lines, lastSequence });
                setFollow(false);
              }}
            >
              Pause
            </Button>
          )}
          <Button variant="ghost" aria-pressed={follow} onClick={() => setFollow((f) => !f)}>
            Auto-scroll {follow ? "on" : "off"}
          </Button>
        </span>
      </header>
      {entry?.error && (
        <div className="p-3">
          <ErrorPanel error={entry.error} onRetry={() => void start(key, device.serial, source)} />
        </div>
      )}
      <div
        ref={scroller}
        onScroll={onScroll}
        onKeyDown={onLogKeyDown}
        className="relative min-h-0 flex-1 overflow-auto"
        role="log"
        aria-label="Log output"
        aria-live={paused ? "off" : "polite"}
      >
        <div style={{ height: virt.getTotalSize(), position: "relative" }}>
          {virt.getVirtualItems().map((v) => {
            const line = renderedLines[v.index];
            return line ? (
              <div
                key={line.seq}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  right: 0,
                  transform: `translateY(${v.start}px)`,
                }}
              >
                <LogRow
                  line={line}
                  pattern={searchPattern}
                  activeMatch={line.seq === activeMatchSequence}
                  selected={selectedSeqs.has(line.seq)}
                  onSelect={(event) => selectLine(event, line, v.index)}
                />
              </div>
            ) : null;
          })}
        </div>
      </div>
      {!follow && lines.length > 0 && (
        <div className="border-t border-border px-4 py-2">
          <Button onClick={() => setFollow(true)}>Jump to latest</Button>
        </div>
      )}
      {exportStatus && <p className="sr-only" role="status">{exportStatus}</p>}
      <Dialog
        open={exportDialogOpen}
        title="Export logs"
        onClose={() => setExportDialogOpen(false)}
        footer={
          <>
            <Button onClick={() => setExportDialogOpen(false)}>Cancel</Button>
            <Button variant="primary" onClick={() => void exportLogs()}>
              Choose save location
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <label className="block text-[12px] text-muted" htmlFor="logs-export-scope">
            Lines
          </label>
          <select
            id="logs-export-scope"
            value={exportScope}
            onChange={(event) => setExportScope(event.target.value as "visible" | "all")}
            className="w-full rounded border border-border bg-bg px-2 py-1.5 text-[12px] text-text"
          >
            <option value="visible">Filtered visible lines</option>
            <option value="all">All buffered lines</option>
          </select>
          <label className="block text-[12px] text-muted" htmlFor="logs-export-format">
            Format
          </label>
          <select
            id="logs-export-format"
            value={exportFormat}
            onChange={(event) => setExportFormat(event.target.value as LogExportFormat)}
            className="w-full rounded border border-border bg-bg px-2 py-1.5 text-[12px] text-text"
          >
            <option value="log">Raw .log</option>
            <option value="jsonl">Structured .jsonl</option>
          </select>
        </div>
      </Dialog>
    </div>
  );
}
