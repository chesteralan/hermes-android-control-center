import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useDevices } from "../../stores/devices";
import { MAX_LOG_LINES, useLogs } from "../../stores/logs";
import { device } from "../../test/fixtures";
import { mockIpc, resetStores } from "../../test/ipcMock";
import type { LogLine } from "../../types";
import { LogAutoStart } from "./LogAutoStart";
import {
  searchLogSequences,
  type LogSearchWorkerRequest,
  type LogSearchWorkerResponse,
} from "./logSearch.worker";
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

  it("starts the selected Hermes source and clears lines from the prior source", async () => {
    useLogs.setState({
      byDevice: {
        [KEY]: {
          lines: [line(1)],
          source: "logcat",
          streamId: null,
          starting: false,
          error: null,
        },
      },
    });
    const fn = mockIpc({ start_log_stream: () => "hermes-logs-1" });
    render(<LogsView />);

    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Log source" }), "hermesGateway");
    await userEvent.click(screen.getByRole("button", { name: "Start" }));

    expect(fn).toHaveBeenCalledWith(
      "start_log_stream",
      expect.objectContaining({ serial: device().serial, source: "hermesGateway" }),
    );
    expect(useLogs.getState().byDevice[KEY]?.source).toBe("hermesGateway");
    expect(useLogs.getState().byDevice[KEY]?.lines).toEqual([]);
    expect(screen.getByRole("combobox", { name: "Log source" })).toBeDisabled();
  });

  it("buffers incoming lines while paused and resumes at the latest line", async () => {
    let channel: { onmessage: (batch: LogLine[]) => void } | undefined;
    mockIpc({
      start_log_stream: (args) => {
        channel = args?.onBatch as typeof channel;
        return "pause-logs";
      },
    });
    const offsetHeightSpy = vi
      .spyOn(HTMLElement.prototype, "offsetHeight", "get")
      .mockImplementation(function (this: HTMLElement) {
        return this.getAttribute("aria-label") === "Log output" ? 600 : 0;
      });
    try {
      render(<LogsView />);
      await userEvent.click(screen.getByRole("button", { name: "Start" }));
      channel?.onmessage([line(1), line(2)]);
      expect(await screen.findByText("message 2")).toBeInTheDocument();

      await userEvent.click(screen.getByRole("button", { name: "Pause" }));
      channel?.onmessage([line(3), line(4)]);

      expect(await screen.findByText(/4 lines/)).toBeInTheDocument();
      expect(screen.getByText("message 2")).toBeInTheDocument();
      expect(screen.queryByText("message 3")).not.toBeInTheDocument();
      const resume = await screen.findByRole("button", { name: "Resume · 2 new" });
      await userEvent.click(resume);

      expect(await screen.findByText("message 4")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Auto-scroll on" })).toHaveAttribute(
        "aria-pressed",
        "true",
      );
    } finally {
      offsetHeightSpy.mockRestore();
    }
  });

  it("searches text and regex, toggles case, highlights matches, and reports invalid regex", async () => {
    mockIpc({});
    useLogs.getState().append(KEY, [
      line(1, { tag: "App", message: "Hermes started" }),
      line(2, { tag: "Queue", level: "warn", message: "queue warning" }),
      line(3, { tag: "App", message: "Hermes stopped" }),
    ]);
    const offsetHeightSpy = vi
      .spyOn(HTMLElement.prototype, "offsetHeight", "get")
      .mockImplementation(function (this: HTMLElement) {
        return this.getAttribute("aria-label") === "Log output" ? 600 : 0;
      });
    try {
      const { container } = render(<LogsView />);
      const search = screen.getByRole("searchbox", { name: "Search logs" });
      await userEvent.type(search, "hermes");
      expect(await screen.findByText("2 matches")).toBeInTheDocument();
      expect(Array.from(container.querySelectorAll("mark")).map((mark) => mark.textContent)).toEqual([
        "Hermes",
        "Hermes",
      ]);
      expect(screen.queryByText("queue warning")).not.toBeInTheDocument();
      expect(screen.getByText(/3 lines/)).toBeInTheDocument();

      await userEvent.click(screen.getByRole("checkbox", { name: "Case sensitive" }));
      expect(await screen.findByText("0 matches")).toBeInTheDocument();
      await userEvent.click(screen.getByRole("checkbox", { name: "Case sensitive" }));
      expect(await screen.findByText("2 matches")).toBeInTheDocument();

      await userEvent.clear(search);
      await userEvent.type(search, "warning");
      await userEvent.click(screen.getByRole("checkbox", { name: "Use regular expression" }));
      expect(await screen.findByText("1 matches")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Next log match" })).toBeEnabled();
      expect(container.querySelectorAll("mark")).toHaveLength(1);

      await userEvent.clear(search);
      fireEvent.change(search, { target: { value: "[" } });
      expect(await screen.findByRole("alert")).toHaveTextContent("Invalid regular expression");
      expect(useLogs.getState().byDevice[KEY]?.lines).toHaveLength(3);
    } finally {
      offsetHeightSpy.mockRestore();
    }
  });

  it("uses the Worker search path above 10,000 visible lines", async () => {
    mockIpc({});
    useLogs.getState().append(
      KEY,
      Array.from({ length: 10_001 }, (_, index) =>
        line(index + 1, {
          tag: "WorkerTest",
          message: index === 10_000 ? "worker target match" : `bulk line ${index}`,
        }),
      ),
    );
    class MockWorker {
      onmessage: ((event: MessageEvent<LogSearchWorkerResponse>) => void) | null = null;
      constructor(readonly scriptUrl: URL, readonly options: WorkerOptions) {}

      postMessage(request: LogSearchWorkerRequest) {
        this.onmessage?.({ data: searchLogSequences(request) } as MessageEvent<LogSearchWorkerResponse>);
      }

      terminate() {}
    }

    vi.stubGlobal("Worker", MockWorker);
    const offsetHeightSpy = vi
      .spyOn(HTMLElement.prototype, "offsetHeight", "get")
      .mockImplementation(function (this: HTMLElement) {
        return this.getAttribute("aria-label") === "Log output" ? 600 : 0;
      });
    try {
      const { container } = render(<LogsView />);
      const search = screen.getByRole("searchbox", { name: "Search logs" });
      await userEvent.type(search, "worker target");

      expect(await screen.findByText("1 matches", {}, { timeout: 3000 })).toBeInTheDocument();
      expect(container.querySelector('[data-seq="10001"]')).toHaveTextContent("worker target match");
      expect(screen.queryByText(/bulk line/)).not.toBeInTheDocument();
      expect(useLogs.getState().byDevice[KEY]?.lines).toHaveLength(10_001);

      expect(
        searchLogSequences({
          requestId: 99,
          signature: "literal-search",
          query: "[",
          regex: false,
          caseSensitive: false,
          lines: [
            { seq: 1, text: "literal [ bracket" },
            { seq: 2, text: "no bracket" },
          ],
        }).matchedSeqs,
      ).toEqual([1]);
    } finally {
      offsetHeightSpy.mockRestore();
      vi.unstubAllGlobals();
    }
  });

  it("filters by level with counts and restores all levels", async () => {
    mockIpc({});
    useLogs.getState().append(KEY, [
      line(1, { level: "info", message: "information" }),
      line(2, { level: "warn", message: "warning" }),
      line(3, { level: "error", message: "failure" }),
      line(4, { level: "debug", message: "diagnostic" }),
      line(5, { level: null, message: "plain" }),
    ]);
    const offsetHeightSpy = vi
      .spyOn(HTMLElement.prototype, "offsetHeight", "get")
      .mockImplementation(function (this: HTMLElement) {
        return this.getAttribute("aria-label") === "Log output" ? 600 : 0;
      });
    try {
      render(<LogsView />);
      expect(screen.getByRole("button", { name: "INFO 1" })).toHaveAttribute("aria-pressed", "true");
      expect(screen.getByRole("button", { name: "WARN 1" })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "ERROR 1" })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "DEBUG 1" })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "UNKNOWN 1" })).toBeInTheDocument();

      await userEvent.click(screen.getByRole("button", { name: "INFO 1" }));
      await userEvent.click(screen.getByRole("button", { name: "ERROR 1" }));
      await userEvent.click(screen.getByRole("button", { name: "DEBUG 1" }));
      await userEvent.click(screen.getByRole("button", { name: "UNKNOWN 1" }));
      expect(screen.getByText("warning")).toBeInTheDocument();
      expect(screen.queryByText("information")).not.toBeInTheDocument();
      expect(screen.queryByText("failure")).not.toBeInTheDocument();
      expect(useLogs.getState().byDevice[KEY]?.lines).toHaveLength(5);

      await userEvent.click(screen.getByRole("button", { name: "All" }));
      expect(screen.getByText("information")).toBeInTheDocument();
      expect(screen.getByText("failure")).toBeInTheDocument();
    } finally {
      offsetHeightSpy.mockRestore();
    }
  });

  it("selects single/range rows and copies selected or visible raw lines", async () => {
    mockIpc({});
    useLogs.getState().append(KEY, [
      line(1, { raw: "raw one", message: "first" }),
      line(2, { raw: "raw two", message: "second" }),
      line(3, { raw: "raw three", message: "third" }),
    ]);
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    const offsetHeightSpy = vi
      .spyOn(HTMLElement.prototype, "offsetHeight", "get")
      .mockImplementation(function (this: HTMLElement) {
        return this.getAttribute("aria-label") === "Log output" ? 600 : 0;
      });

    try {
      render(<LogsView />);
      const rowOne = screen.getByRole("button", { name: "Select log line 1" });
      const rowTwo = screen.getByRole("button", { name: "Select log line 2" });
      const rowThree = screen.getByRole("button", { name: "Select log line 3" });
      await userEvent.click(rowOne);
      expect(rowOne).toHaveAttribute("aria-pressed", "true");
      fireEvent.click(rowThree, { shiftKey: true });
      expect(rowTwo).toHaveAttribute("aria-pressed", "true");
      expect(rowThree).toHaveAttribute("aria-pressed", "true");

      fireEvent.click(rowTwo, { ctrlKey: true });
      expect(rowTwo).toHaveAttribute("aria-pressed", "false");
      await userEvent.keyboard("{Control>}c{/Control}");
      expect(writeText).toHaveBeenCalledWith("raw one\nraw three");

      await userEvent.click(screen.getByRole("button", { name: "Copy visible" }));
      expect(writeText).toHaveBeenLastCalledWith("raw one\nraw two\nraw three");
    } finally {
      offsetHeightSpy.mockRestore();
    }
  });

  it("exports filtered JSONL or all buffered raw logs through Rust", async () => {
    const exportRequests: Array<Record<string, unknown>> = [];
    mockIpc({
      export_logs: (args) => {
        if (args) exportRequests.push(args);
        return "/tmp/hermes-logs.txt";
      },
    });
    const sourceLines = [
      line(1, { message: "keep this", raw: "raw keep" }),
      line(2, { message: "discard this", raw: "raw discard" }),
    ];
    useLogs.getState().append(KEY, sourceLines);

    render(<LogsView />);
    await userEvent.type(screen.getByRole("searchbox", { name: "Search logs" }), "keep");
    await screen.findByText("1 matches");
    await userEvent.click(screen.getByRole("button", { name: "Export" }));
    const exportDialog = screen.getByRole("dialog", { name: "Export logs" });
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Format" }),
      "jsonl",
    );
    await userEvent.click(screen.getByRole("button", { name: "Choose save location" }));
    await waitFor(() => expect(exportRequests).toHaveLength(1));
    expect(exportRequests[0]).toMatchObject({ lines: [sourceLines[0]], format: "jsonl" });
    expect(exportDialog).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Export" }));
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Lines" }),
      "all",
    );
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Format" }),
      "log",
    );
    await userEvent.click(screen.getByRole("button", { name: "Choose save location" }));
    await waitFor(() => expect(exportRequests).toHaveLength(2));
    expect(exportRequests[1]).toMatchObject({ lines: sourceLines, format: "log" });
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

describe("LogAutoStart", () => {
  beforeEach(() => {
    resetStores();
    useLogs.setState({ byDevice: {} });
    useDevices.getState().setDevices([device()]);
  });

  it("starts Hermes gateway logs after the connected device passes the Termux check", async () => {
    const fn = mockIpc({
      check_termux: () => ({ ready: true, items: [], distros: ["debian"] }),
      start_log_stream: () => "auto-hermes-logs",
    });
    render(<LogAutoStart enabled />);

    await waitFor(() =>
      expect(fn).toHaveBeenCalledWith(
        "start_log_stream",
        expect.objectContaining({ serial: device().serial, source: "hermesGateway" }),
      ),
    );
    expect(fn).toHaveBeenCalledWith("check_termux", { serial: device().serial });
  });

  it("does not start logs when disabled or Termux is not ready", async () => {
    const fn = mockIpc({
      check_termux: () => ({ ready: false, items: [], distros: [] }),
      start_log_stream: () => "unexpected-logs",
    });
    const { rerender } = render(<LogAutoStart enabled={false} />);
    await Promise.resolve();
    expect(fn).not.toHaveBeenCalledWith("check_termux", expect.anything());

    rerender(<LogAutoStart enabled />);
    await waitFor(() => expect(fn).toHaveBeenCalledWith("check_termux", { serial: device().serial }));
    expect(fn).not.toHaveBeenCalledWith("start_log_stream", expect.anything());
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
