import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useRef, useState } from "react";
import { Button } from "../../components/Button";
import { ErrorPanel } from "../../components/ErrorPanel";
import { deviceKey, useActiveDevice } from "../../stores/devices";
import { useLogs } from "../../stores/logs";
import type { LogLevel, LogLine } from "../../types";
import { NoDeviceState } from "../device/NoDeviceState";

const ROW = 20;

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

export function LogRow({ line }: { line: LogLine }) {
  return (
    <div
      className="flex gap-3 whitespace-pre px-3 font-mono text-[12px]"
      style={{ height: ROW, lineHeight: `${ROW}px` }}
    >
      <span className="w-[140px] shrink-0 text-muted">{line.timestamp ?? ""}</span>
      <LevelBadge level={line.level} />
      <span className="truncate">
        {line.tag && <span className="text-muted">{line.tag}: </span>}
        {line.message}
      </span>
    </div>
  );
}

export function LogsView() {
  const device = useActiveDevice();
  const key = device ? deviceKey(device) : "";
  const entry = useLogs((s) => s.byDevice[key]);
  const { start, stop, clear } = useLogs();
  const lines = entry?.lines ?? [];
  const streaming = !!entry?.streamId;
  const [follow, setFollow] = useState(true);
  const scroller = useRef<HTMLDivElement>(null);

  // eslint-disable-next-line react-hooks/incompatible-library -- virtualizer state is read during render by design
  const virt = useVirtualizer({
    count: lines.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW,
    overscan: 20,
  });

  useEffect(() => {
    if (follow && lines.length > 0) virt.scrollToIndex(lines.length - 1, { align: "end" });
  }, [follow, lines.length, virt]);

  if (!device) return <NoDeviceState />;

  const onScroll = () => {
    const el = scroller.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < ROW * 2;
    if (atBottom !== follow) setFollow(atBottom);
  };

  return (
    <div className="flex h-[calc(100vh-44px-48px)] flex-col rounded-lg border border-border bg-surface">
      <header className="flex items-center justify-between border-b border-border px-4 py-2">
        <span>
          <b>Logs</b>{" "}
          <span className="text-muted">Android logcat · {lines.length.toLocaleString()} lines</span>
        </span>
        <span className="flex gap-2">
          {streaming ? (
            <Button onClick={() => void stop(key)}>Stop</Button>
          ) : (
            <Button
              variant="primary"
              loading={entry?.starting}
              onClick={() => void start(key, device.serial)}
            >
              Start
            </Button>
          )}
          <Button variant="ghost" onClick={() => clear(key)}>
            Clear
          </Button>
          <Button variant="ghost" aria-pressed={follow} onClick={() => setFollow((f) => !f)}>
            Auto-scroll {follow ? "on" : "off"}
          </Button>
        </span>
      </header>
      {entry?.error && (
        <div className="p-3">
          <ErrorPanel error={entry.error} onRetry={() => void start(key, device.serial)} />
        </div>
      )}
      <div
        ref={scroller}
        onScroll={onScroll}
        className="relative flex-1 overflow-auto"
        role="log"
        aria-label="Log output"
      >
        <div style={{ height: virt.getTotalSize(), position: "relative" }}>
          {virt.getVirtualItems().map((v) => {
            const line = lines[v.index];
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
                <LogRow line={line} />
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
    </div>
  );
}
