import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useDevices } from "../../stores/devices";
import { MAX_LOG_LINES, useLogs } from "../../stores/logs";
import { device } from "../../test/fixtures";
import { mockIpc, resetStores } from "../../test/ipcMock";
import type { LogLine } from "../../types";
import { LogsView } from "./LogsView";
import { RecentLogs } from "./RecentLogs";

const KEY = "TESTSERIAL0001";

function line(seq: number, over: Partial<LogLine> = {}): LogLine {
  return {
    seq,
    receivedAt: 0,
    timestamp: "10-02 16:11:21.369",
    level: "info",
    tag: "Hermes",
    message: `message ${seq}`,
    raw: `raw ${seq}`,
    ...over,
  };
}

describe("logs store", () => {
  beforeEach(() => useLogs.setState({ byDevice: {} }));

  it("appends batches and keeps a bounded ring buffer per device", () => {
    const { append } = useLogs.getState();
    append(KEY, [line(1), line(2)]);
    append("other", [line(1)]);
    expect(useLogs.getState().byDevice[KEY]?.lines).toHaveLength(2);
    append(
      KEY,
      Array.from({ length: MAX_LOG_LINES }, (_, i) => line(i + 3)),
    );
    const lines = useLogs.getState().byDevice[KEY]?.lines ?? [];
    expect(lines).toHaveLength(MAX_LOG_LINES);
    expect(lines[0]?.seq).toBe(3);
    expect(useLogs.getState().byDevice.other?.lines).toHaveLength(1);
  });

  it("clears", () => {
    useLogs.getState().append(KEY, [line(1)]);
    useLogs.getState().clear(KEY);
    expect(useLogs.getState().byDevice[KEY]?.lines).toHaveLength(0);
  });
});

describe("LogsView", () => {
  beforeEach(() => {
    resetStores();
    useLogs.setState({ byDevice: {} });
    useDevices.getState().setDevices([device()]);
  });

  it("starts logcat, receives batches and stops", async () => {
    let chan: { onmessage: (b: LogLine[]) => void } | undefined;
    const fn = mockIpc({
      start_log_stream: (args) => {
        chan = args?.onBatch as typeof chan;
        return "logs-1";
      },
      cancel_stream: () => true,
    });
    render(<LogsView />);
    await userEvent.click(screen.getByRole("button", { name: "Start" }));
    expect(fn).toHaveBeenCalledWith(
      "start_log_stream",
      expect.objectContaining({ serial: device().serial, source: "logcat" }),
    );
    chan?.onmessage([line(1), line(2), line(3)]);
    expect(await screen.findByText(/3 lines/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(fn).toHaveBeenCalledWith("cancel_stream", { streamId: "logs-1" });
  });

  it("toggles auto-scroll and clears", async () => {
    mockIpc({});
    useLogs.getState().append(KEY, [line(1)]);
    render(<LogsView />);
    const toggle = screen.getByRole("button", { name: /Auto-scroll on/ });
    await userEvent.click(toggle);
    expect(screen.getByRole("button", { name: /Auto-scroll off/ })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(screen.getByRole("button", { name: "Jump to latest" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(await screen.findByText(/0 lines/)).toBeInTheDocument();
  });

  it("shows start errors", async () => {
    mockIpc({
      start_log_stream: () => {
        throw { kind: "deviceOffline", message: "Device is offline.", details: null };
      },
    });
    render(<LogsView />);
    await userEvent.click(screen.getByRole("button", { name: "Start" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("offline");
  });
});

describe("RecentLogs", () => {
  beforeEach(() => {
    resetStores();
    useLogs.setState({ byDevice: {} });
    useDevices.getState().setDevices([device()]);
  });

  it("shows the last 20 lines, preserving unstructured text", () => {
    useLogs.getState().append(
      KEY,
      Array.from({ length: 30 }, (_, i) =>
        line(i + 1, i === 29 ? { level: null, tag: null, message: "plain text" } : {}),
      ),
    );
    render(<RecentLogs />);
    expect(screen.queryByText("message 10")).not.toBeInTheDocument();
    expect(screen.getByText("message 11")).toBeInTheDocument();
    expect(screen.getByText("plain text")).toBeInTheDocument();
  });
});
