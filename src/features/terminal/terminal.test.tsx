import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { deviceKey, useDevices } from "../../stores/devices";
import {
  MAX_COMMAND_HISTORY,
  MAX_TERMINAL_LINES,
  terminalHistoryScope,
  terminalSessionKey,
  trimTerminalBlocks,
  useTerminal,
  type CommandBlock,
} from "../../stores/terminal";
import { device } from "../../test/fixtures";
import { mockIpc, resetStores } from "../../test/ipcMock";
import type { StreamEvent } from "../../types";
import { AnsiText } from "./AnsiText";
import { TerminalView } from "./TerminalView";

vi.mock("./InteractiveTerminal", () => ({
  InteractiveTerminal: ({ serial, onClose }: { serial: string; onClose: () => void }) => (
    <section aria-label="Interactive shell">
      <span>{serial}</span>
      <button type="button" onClick={onClose} aria-label="Close interactive shell">
        Close
      </button>
    </section>
  ),
}));

type Chan = { onmessage: (e: StreamEvent) => void };

describe("TerminalView", () => {
  beforeEach(() => {
    localStorage.clear();
    resetStores();
    useTerminal.setState({
      sessions: {},
      tabsByDevice: {},
      activeSessionByDevice: {},
      snippets: [],
      historyByScope: {},
      persistedHistoryByScope: {},
      historyPersistenceEnabled: true,
    });
    useDevices.getState().setDevices([device()]);
  });

  it("is labelled as the Android shell, not Termux", () => {
    mockIpc({});
    render(<TerminalView />);
    expect(screen.getByText(/runs as the shell user, not Termux/)).toBeInTheDocument();
  });

  it("opens an interactive shell only in Termux mode and can close it", async () => {
    mockIpc({});
    render(<TerminalView />);
    expect(screen.queryByRole("button", { name: "Interactive shell" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("radio", { name: "Termux" }));
    await userEvent.click(screen.getByRole("button", { name: "Interactive shell" }));
    expect(await screen.findByRole("region", { name: "Interactive shell" })).toHaveTextContent(
      "adb-TESTSERIAL0001-a00nZY._adb-tls-connect._tcp",
    );

    await userEvent.click(
      within(screen.getByRole("region", { name: "Interactive shell" })).getByRole("button", {
        name: "Close interactive shell",
      }),
    );
    expect(screen.queryByRole("region", { name: "Interactive shell" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("radio", { name: "Android shell" }));
    expect(screen.queryByRole("button", { name: "Interactive shell" })).not.toBeInTheDocument();
  });

  it("only runs explicitly submitted commands and renders streamed output", async () => {
    let chan: Chan | undefined;
    const fn = mockIpc({
      stream_command: (args) => {
        chan = args?.onEvent as Chan;
        return "cmd-1";
      },
    });
    render(<TerminalView />);
    expect(fn).not.toHaveBeenCalled();
    await userEvent.type(screen.getByLabelText("Command"), "ls /sdcard{Enter}");
    expect(fn).toHaveBeenCalledWith(
      "stream_command",
      expect.objectContaining({ serial: device().serial, command: "ls /sdcard" }),
    );
    chan?.onmessage({ type: "stdout", line: "Download" });
    chan?.onmessage({ type: "stderr", line: "ls: x: Permission denied" });
    chan?.onmessage({ type: "exit", code: 1, durationMs: 42 });
    expect(await screen.findByText("Download")).toHaveAttribute("data-stream", "out");
    expect(screen.getByText("ls: x: Permission denied")).toHaveAttribute("data-stream", "err");
    expect(screen.getByText("exit 1 · 42 ms")).toBeInTheDocument();
  });

  it("keeps the UI responsive during a long stream and reloads command history", async () => {
    const fn = mockIpc({ stream_command: () => "long-ping", cancel_stream: () => true });
    render(<TerminalView />);
    await userEvent.type(screen.getByRole("textbox", { name: "Command" }), "ping -c 100{Enter}");
    await screen.findByRole("button", { name: "Cancel" });

    await userEvent.click(screen.getByRole("button", { name: "New terminal session" }));
    expect(await screen.findByRole("tab", { name: "Terminal 2" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("textbox", { name: "Command" })).toBeEnabled();

    const scope = terminalHistoryScope(deviceKey(device()), "adbShell");
    act(() => {
      useTerminal.setState({ historyByScope: {}, persistedHistoryByScope: {} });
      useTerminal.getState().loadCommandHistory(scope);
    });
    expect(useTerminal.getState().historyByScope[scope]).toContain("ping -c 100");

    await userEvent.click(screen.getByRole("tab", { name: "Terminal 1" }));
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(fn).toHaveBeenCalledWith("cancel_stream", { streamId: "long-ping" });
    expect(await screen.findByText("cancelled")).toBeInTheDocument();
  });

  it("navigates command history and reverse-searches with Ctrl+R", async () => {
    let streamNumber = 0;
    const fn = mockIpc({
      stream_command: (args) => {
        const channel = args?.onEvent as Chan;
        channel.onmessage({ type: "exit", code: 0, durationMs: 1 });
        streamNumber += 1;
        return `history-${streamNumber}`;
      },
    });
    render(<TerminalView />);
    const input = screen.getByRole("textbox", { name: "Command" });
    await userEvent.type(input, "echo alpha{Enter}");
    await userEvent.type(input, "echo beta{Enter}");

    await userEvent.keyboard("{ArrowUp}");
    expect(input).toHaveValue("echo beta");
    await userEvent.keyboard("{ArrowUp}");
    expect(input).toHaveValue("echo alpha");
    await userEvent.keyboard("{ArrowDown}");
    expect(input).toHaveValue("echo beta");
    await userEvent.keyboard("{ArrowDown}");
    expect(input).toHaveValue("");

    await userEvent.type(input, "echo");
    await userEvent.keyboard("{Control>}r{/Control}");
    expect(screen.getByRole("status")).toHaveTextContent("Reverse search: echo beta");
    await userEvent.keyboard("{Control>}r{/Control}");
    expect(screen.getByRole("status")).toHaveTextContent("Reverse search: echo alpha");
    await userEvent.keyboard("{Enter}");
    expect(input).toHaveValue("echo alpha");
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it("deduplicates and caps history per device and transport, and disables persistence", () => {
    const shellScope = terminalHistoryScope(device().deviceId ?? device().serial, "adbShell");
    const termuxScope = terminalHistoryScope(device().deviceId ?? device().serial, "termuxSsh");
    const { recordCommand, setHistoryPersistence } = useTerminal.getState();
    recordCommand(shellScope, "repeat");
    recordCommand(shellScope, "repeat");
    expect(useTerminal.getState().historyByScope[shellScope]).toEqual(["repeat"]);

    for (let index = 0; index <= MAX_COMMAND_HISTORY; index += 1) {
      useTerminal.getState().recordCommand(shellScope, `command ${index}`);
    }
    useTerminal.getState().recordCommand(termuxScope, "whoami");

    const shellHistory = useTerminal.getState().historyByScope[shellScope] ?? [];
    expect(shellHistory).toHaveLength(MAX_COMMAND_HISTORY);
    expect(shellHistory[0]).toBe("command 1");
    expect(shellHistory.at(-1)).toBe(`command ${MAX_COMMAND_HISTORY}`);
    expect(useTerminal.getState().historyByScope[termuxScope]).toEqual(["whoami"]);
    expect(JSON.parse(localStorage.getItem(`hacc:terminal:history:${shellScope}`) ?? "[]")).toEqual(
      shellHistory,
    );

    useTerminal.setState({ historyByScope: {}, persistedHistoryByScope: {} });
    useTerminal.getState().loadCommandHistory(shellScope);
    expect(useTerminal.getState().historyByScope[shellScope]).toEqual(shellHistory);

    setHistoryPersistence(false);
    expect(localStorage.getItem(`hacc:terminal:history:${shellScope}`)).toBeNull();
    useTerminal.getState().recordCommand(shellScope, "session only");
    expect(useTerminal.getState().historyByScope[shellScope]?.at(-1)).toBe("session only");
    expect(localStorage.getItem(`hacc:terminal:history:${shellScope}`)).toBeNull();
    expect(useTerminal.getState().historyPersistenceEnabled).toBe(false);
  });

  it("caps a single command block at the terminal scrollback limit", () => {
    const block: CommandBlock = {
      id: 1,
      command: "large-output",
      transport: "adbShell",
      lines: Array.from({ length: MAX_TERMINAL_LINES + 5 }, (_, index) => ({
        stream: "out" as const,
        text: `line ${index}`,
      })),
      running: false,
      streamId: null,
      exitCode: 0,
      durationMs: 1,
      cancelled: false,
      error: null,
    };

    const [trimmed] = trimTerminalBlocks([block]);

    expect(trimmed?.lines).toHaveLength(MAX_TERMINAL_LINES);
    expect(trimmed?.lines[0]?.text).toBe("line 5");
    expect(trimmed?.lines.at(-1)?.text).toBe(`line ${MAX_TERMINAL_LINES + 4}`);
  });

  it("trims exactly the oldest excess across multiple command blocks", () => {
    const makeBlock = (id: number, prefix: string, count: number): CommandBlock => ({
      id,
      command: `command-${id}`,
      transport: "adbShell",
      lines: Array.from({ length: count }, (_, index) => ({
        stream: "out" as const,
        text: `${prefix} ${index}`,
      })),
      running: false,
      streamId: null,
      exitCode: 0,
      durationMs: 1,
      cancelled: false,
      error: null,
    });

    const blocks = trimTerminalBlocks([
      makeBlock(1, "old", 9_000),
      makeBlock(2, "new", 1_001),
    ]);

    expect(blocks.reduce((total, block) => total + block.lines.length, 0)).toBe(MAX_TERMINAL_LINES);
    expect(blocks[0]?.lines).toHaveLength(8_999);
    expect(blocks[0]?.lines[0]?.text).toBe("old 1");
    expect(blocks[1]?.lines[0]?.text).toBe("new 0");
  });

  it("batches 1,000 streamed lines per second without dropping or reordering output", async () => {
    vi.useFakeTimers();
    try {
      let channel: Chan | undefined;
      mockIpc({
        stream_command: (args) => {
          channel = args?.onEvent as Chan;
          return "throughput-stream";
        },
      });
      const currentDevice = device();
      const key = deviceKey(currentDevice);
      await useTerminal
        .getState()
        .run(key, currentDevice.serial, "stream-benchmark", "adbShell");
      const sessionId = useTerminal.getState().activeSessionByDevice[key];
      if (!sessionId || !channel) throw new Error("Terminal stream did not start");
      const sessionKey = terminalSessionKey(key, sessionId);
      let lineUpdates = 0;
      const unsubscribe = useTerminal.subscribe((state, previous) => {
        const currentCount =
          state.sessions[sessionKey]?.blocks[0]?.lines.length ?? 0;
        const previousCount =
          previous.sessions[sessionKey]?.blocks[0]?.lines.length ?? 0;
        if (currentCount > previousCount) lineUpdates += 1;
      });

      await act(async () => {
        for (let batch = 0; batch < 63; batch += 1) {
          for (let index = 0; index < 16; index += 1) {
            const sequence = batch * 16 + index;
            channel?.onmessage({ type: "stdout", line: `line ${sequence}` });
          }
          await vi.advanceTimersByTimeAsync(16);
        }
      });
      channel.onmessage({ type: "exit", code: 0, durationMs: 160 });
      unsubscribe();

      const lines = useTerminal.getState().sessions[sessionKey]?.blocks[0]?.lines ?? [];
  expect(lines).toHaveLength(1008);
      expect(lines[0]?.text).toBe("line 0");
  expect(lines.at(-1)?.text).toBe("line 1007");
  expect(lineUpdates).toBe(63);
    } finally {
      vi.useRealTimers();
    }
  });

  it("cancels a stream after its configured command timeout", async () => {
    vi.useFakeTimers();
    try {
      const fn = mockIpc({ stream_command: () => "timeout-stream", cancel_stream: () => true });
      const currentDevice = device();
      const key = deviceKey(currentDevice);
      await useTerminal
        .getState()
        .run(key, currentDevice.serial, "long-command", "adbShell", undefined, 1_000);
      const sessionId = useTerminal.getState().activeSessionByDevice[key];
      if (!sessionId) throw new Error("Terminal session did not start");

      await act(async () => {
        await vi.advanceTimersByTimeAsync(1_000);
      });

      expect(fn).toHaveBeenCalledWith("cancel_stream", { streamId: "timeout-stream" });
      expect(
        useTerminal.getState().sessions[terminalSessionKey(key, sessionId)]?.blocks[0]?.cancelled,
      ).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it("virtualizes large terminal scrollback", async () => {
    const currentDevice = device();
    const block: CommandBlock = {
      id: 1,
      command: "large-output",
      transport: "adbShell",
      lines: Array.from({ length: MAX_TERMINAL_LINES }, (_, index) => ({
        stream: "out" as const,
        text: `terminal line ${index}`,
      })),
      running: false,
      streamId: null,
      exitCode: 0,
      durationMs: 1,
      cancelled: false,
      error: null,
    };
    const currentDeviceKey = deviceKey(currentDevice);
    const sessionId = useTerminal.getState().createSession(currentDeviceKey);
    useTerminal.setState((state) => ({
      sessions: {
        ...state.sessions,
        [terminalSessionKey(currentDeviceKey, sessionId)]: { blocks: [block] },
      },
    }));
    const offsetHeightSpy = vi
      .spyOn(HTMLElement.prototype, "offsetHeight", "get")
      .mockImplementation(function (this: HTMLElement) {
        if (this.getAttribute("aria-label") === "Terminal output") return 600;
        return this.hasAttribute("data-index") ? 24 : 0;
      });

    try {
      render(<TerminalView />);
      const visibleLines = await screen.findAllByText(/terminal line/);
      expect(visibleLines.length).toBeGreaterThan(0);
      expect(visibleLines.length).toBeLessThan(MAX_TERMINAL_LINES);
    } finally {
      offsetHeightSpy.mockRestore();
    }
  });

  it("runs in Termux over SSH when selected and labels the block", async () => {
    const fn = mockIpc({ stream_command: () => "cmd-9" });
    render(<TerminalView />);
    await userEvent.click(screen.getByRole("radio", { name: "Termux" }));
    expect(screen.getByText(/runs as the Termux user/)).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Command"), "whoami{Enter}");
    expect(fn).toHaveBeenCalledWith(
      "stream_command",
      expect.objectContaining({ command: "whoami", transportKind: "termuxSsh" }),
    );
    const log = screen.getByRole("log");
    expect(log).toHaveTextContent("Termux");
  });

  it("keeps output independent across terminal tabs and supports Mod+T/Mod+W", async () => {
    const channels: Chan[] = [];
    mockIpc({
      stream_command: (args) => {
        const channel = args?.onEvent as Chan;
        channels.push(channel);
        return `tab-stream-${channels.length}`;
      },
    });

    render(<TerminalView />);
    const firstTab = await screen.findByRole("tab", { name: "Terminal 1" });
    const input = screen.getByRole("textbox", { name: "Command" });
    await userEvent.type(input, "printf first{Enter}");
    channels[0]?.onmessage({ type: "stdout", line: "first session output" });
    channels[0]?.onmessage({ type: "exit", code: 0, durationMs: 1 });
    expect(await screen.findByText("first session output")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "New terminal session" }));
    const secondTab = await screen.findByRole("tab", { name: "Terminal 2" });
    expect(secondTab).toHaveAttribute("aria-selected", "true");
    expect(screen.queryByText("first session output")).not.toBeInTheDocument();

    await userEvent.type(input, "printf second{Enter}");
    channels[1]?.onmessage({ type: "stdout", line: "second session output" });
    channels[1]?.onmessage({ type: "exit", code: 0, durationMs: 1 });
    expect(await screen.findByText("second session output")).toBeInTheDocument();

    await userEvent.click(firstTab);
    expect(screen.getByText("first session output")).toBeInTheDocument();
    expect(screen.queryByText("second session output")).not.toBeInTheDocument();

    await userEvent.keyboard("{Control>}t{/Control}");
    expect(await screen.findByRole("tab", { name: "Terminal 3" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await userEvent.keyboard("{Control>}w{/Control}");
    expect(screen.queryByRole("tab", { name: "Terminal 3" })).not.toBeInTheDocument();
  });

  it("confirms before closing a terminal tab with a running command", async () => {
    const fn = mockIpc({ stream_command: () => "running-tab-stream", cancel_stream: () => true });
    render(<TerminalView />);
    await screen.findByRole("tab", { name: "Terminal 1" });
    await userEvent.type(screen.getByRole("textbox", { name: "Command" }), "long command{Enter}");
    await screen.findByRole("button", { name: "Cancel" });
    await userEvent.click(screen.getByRole("button", { name: "New terminal session" }));
    await userEvent.click(screen.getByRole("tab", { name: "Terminal 1" }));
    await userEvent.click(screen.getByRole("button", { name: "Close Terminal 1" }));

    const dialog = screen.getByRole("dialog", { name: "Close terminal session?" });
    expect(dialog).toHaveTextContent("Closing this tab cancels its running command.");
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("tab", { name: "Terminal 1" })).toBeInTheDocument();
    expect(fn).not.toHaveBeenCalledWith("cancel_stream", { streamId: "running-tab-stream" });

    await userEvent.click(screen.getByRole("button", { name: "Close Terminal 1" }));
    await userEvent.click(screen.getByRole("button", { name: "Close session" }));
    expect(fn).toHaveBeenCalledWith("cancel_stream", { streamId: "running-tab-stream" });
    expect(screen.queryByRole("tab", { name: "Terminal 1" })).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Terminal 2" })).toHaveAttribute("aria-selected", "true");
  });

  it("saves snippets and requires confirmation before running the exact command", async () => {
    const requests: Array<Record<string, unknown>> = [];
    mockIpc({
      stream_command: (args) => {
        if (!args) throw new Error("stream args were not passed");
        requests.push(args);
        const channel = args.onEvent as Chan;
        channel.onmessage({ type: "exit", code: 0, durationMs: 1 });
        return `snippet-${requests.length}`;
      },
    });
    render(<TerminalView />);

    const commandInput = screen.getByRole("textbox", { name: "Command" });
    await userEvent.type(commandInput, "tail -n 20 /data/log.txt");
    await userEvent.click(screen.getByRole("button", { name: "Save snippet" }));
    const saveDialog = screen.getByRole("dialog", { name: "Save command snippet" });
    await userEvent.type(screen.getByRole("textbox", { name: "Name" }), "Inspect logs");
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Transport" }), "termuxSsh");
    expect(screen.getByRole("textbox", { name: "Snippet command" })).toHaveValue(
      "tail -n 20 /data/log.txt",
    );
    await userEvent.click(screen.getAllByRole("button", { name: "Save snippet" }).at(-1)!);

    expect(saveDialog).not.toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem("hacc:terminal:snippets") ?? "[]")).toMatchObject([
      {
        name: "Inspect logs",
        command: "tail -n 20 /data/log.txt",
        transport: "termuxSsh",
        requiresConfirmation: true,
      },
    ]);
    await userEvent.click(screen.getByRole("button", { name: "Inspect logs" }));
    const confirmDialog = screen.getByRole("dialog", { name: "Run saved command?" });
    expect(confirmDialog).toHaveTextContent("tail -n 20 /data/log.txt");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(requests).toHaveLength(0);

    await userEvent.click(screen.getByRole("button", { name: "Inspect logs" }));
    await userEvent.click(screen.getByRole("button", { name: "Run command" }));
    expect(requests[0]).toMatchObject({
      command: "tail -n 20 /data/log.txt",
      transportKind: "termuxSsh",
    });
  });

  it("cancels a running command", async () => {
    const fn = mockIpc({ stream_command: () => "cmd-7", cancel_stream: () => true });
    render(<TerminalView />);
    await userEvent.type(screen.getByLabelText("Command"), "logcat{Enter}");
    await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(fn).toHaveBeenCalledWith("cancel_stream", { streamId: "cmd-7" });
    expect(await screen.findByText("cancelled")).toBeInTheDocument();
  });

  it("copies block/all output and sends the full text to the native export command", async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    let channel: Chan | undefined;
    const fn = mockIpc({
      stream_command: (args) => {
        channel = args?.onEvent as Chan;
        return "copy-export-stream";
      },
      export_terminal_text: () => "/tmp/terminal-session.txt",
    });
    render(<TerminalView />);
    await userEvent.type(screen.getByRole("textbox", { name: "Command" }), "printf output{Enter}");
    channel?.onmessage({ type: "stdout", line: "first line" });
    channel?.onmessage({ type: "stderr", line: "warning line" });
    const expectedText = "$ printf output\nfirst line\nwarning line";

    await screen.findByText("warning line");
    await userEvent.click(await screen.findByRole("button", { name: "Copy block" }));
    expect(writeText).toHaveBeenCalledWith("first line\nwarning line");
    await userEvent.click(screen.getByRole("button", { name: "Copy all" }));
    expect(writeText).toHaveBeenLastCalledWith(expectedText);
    await userEvent.click(screen.getByRole("button", { name: "Export .txt" }));

    expect(fn).toHaveBeenCalledWith("export_terminal_text", { text: expectedText });
    expect(await screen.findByRole("status")).toHaveTextContent("Terminal output exported");
  });

  it("cancels the running command with Ctrl+C", async () => {
    const fn = mockIpc({ stream_command: () => "cmd-ctrl-c", cancel_stream: () => true });
    render(<TerminalView />);
    const input = screen.getByRole("textbox", { name: "Command" });
    await userEvent.type(input, "ping -c 100{Enter}");
    await screen.findByRole("button", { name: "Cancel" });

    await userEvent.keyboard("{Control>}c{/Control}");

    expect(fn).toHaveBeenCalledWith("cancel_stream", { streamId: "cmd-ctrl-c" });
    expect(await screen.findByText("cancelled")).toBeInTheDocument();
  });

  it("shows device errors from the stream", async () => {
    let chan: Chan | undefined;
    mockIpc({
      stream_command: (args) => {
        chan = args?.onEvent as Chan;
        return "cmd-2";
      },
    });
    render(<TerminalView />);
    await userEvent.type(screen.getByLabelText("Command"), "id{Enter}");
    chan?.onmessage({
      type: "error",
      error: { kind: "deviceOffline", message: "Device is offline.", details: null },
    });
    expect(await screen.findByRole("alert")).toHaveTextContent("Device is offline.");
  });

  it("renders ANSI colors as styled text and strips other escape sequences", () => {
    const { container } = render(
      <AnsiText text={'\u001b[31mred\u001b[0m plain \u001b]0;title\u0007<script>alert(1)</script>'} />,
    );

    expect(screen.getByText("red")).toHaveStyle({ color: "rgb(210, 83, 61)" });
    expect(container.textContent).toContain("<script>alert(1)</script>");
    expect(container.querySelector("script")).toBeNull();
    expect(container.textContent).not.toContain("\u001b");
  });

  it("strips malformed ANSI sequences without losing surrounding text", () => {
    const { container } = render(<AnsiText text={"before\u001b[31"} />);

    expect(container.textContent).toBe("before");
  });
});
