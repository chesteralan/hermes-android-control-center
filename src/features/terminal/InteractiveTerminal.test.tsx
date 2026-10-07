import { act, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockIpc } from "../../test/ipcMock";
import { useTerminal } from "../../stores/terminal";
import type { TerminalPtyEvent } from "../../lib/ipc";
import { InteractiveTerminal } from "./InteractiveTerminal";

const terminalHarness = vi.hoisted(() => ({
  input: undefined as ((data: string) => void) | undefined,
  resize: undefined as ((size: { cols: number; rows: number }) => void) | undefined,
  writes: [] as Array<number[] | string>,
  options: undefined as { disableStdin: boolean } | undefined,
  keyHandler: undefined as ((event: KeyboardEvent) => boolean) | undefined,
  selection: "",
}));

vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    cols = 80;
    rows = 24;
    options = { disableStdin: true };

    constructor() {
      terminalHarness.options = this.options;
    }

    loadAddon() {}
    open() {}
    focus() {}
    dispose() {}
    attachCustomKeyEventHandler(handler: (event: KeyboardEvent) => boolean) {
      terminalHarness.keyHandler = handler;
    }
    hasSelection() {
      return Boolean(terminalHarness.selection);
    }
    getSelection() {
      return terminalHarness.selection;
    }
    write(data: Uint8Array | string) {
      terminalHarness.writes.push(data instanceof Uint8Array ? Array.from(data) : data);
    }
    onData(callback: (data: string) => void) {
      terminalHarness.input = callback;
      return { dispose: vi.fn() };
    }
    onResize(callback: (size: { cols: number; rows: number }) => void) {
      terminalHarness.resize = callback;
      return { dispose: vi.fn() };
    }
  },
}));

vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class {
    fit() {}
  },
}));

interface ChannelMock {
  onmessage: (event: TerminalPtyEvent) => void;
}

describe("InteractiveTerminal", () => {
  beforeEach(() => {
    terminalHarness.input = undefined;
    terminalHarness.resize = undefined;
    terminalHarness.writes = [];
    terminalHarness.options = undefined;
    terminalHarness.keyHandler = undefined;
    terminalHarness.selection = "";
    useTerminal.setState({ sessions: {}, historyByScope: {}, persistedHistoryByScope: {} });
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        disconnect() {}
      },
    );
  });

  it("forwards PTY bytes and resizes without persisting interactive input", async () => {
    let channel: ChannelMock | undefined;
    const invoke = mockIpc({
      start_terminal_pty: (args) => {
        channel = args?.onEvent as ChannelMock;
        return "pty-1";
      },
      write_terminal_pty: () => undefined,
      resize_terminal_pty: () => undefined,
      close_terminal_pty: () => true,
    });
    const { unmount } = render(<InteractiveTerminal serial="phone-1" onClose={vi.fn()} />);

    expect(await screen.findByText("Interactive SSH shell")).toBeInTheDocument();
    expect(terminalHarness.options?.disableStdin).toBe(false);
    expect(invoke).toHaveBeenCalledWith(
      "start_terminal_pty",
      expect.objectContaining({ serial: "phone-1", columns: 80, rows: 24 }),
    );

    act(() => channel?.onmessage({ type: "data", data: [111, 107] }));
    expect(terminalHarness.writes).toContainEqual([111, 107]);

    act(() => terminalHarness.input?.("sudo "));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("write_terminal_pty", {
        sessionId: "pty-1",
        data: [115, 117, 100, 111, 32],
      }),
    );
    act(() => terminalHarness.resize?.({ cols: 100, rows: 35 }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("resize_terminal_pty", {
        sessionId: "pty-1",
        columns: 100,
        rows: 35,
      }),
    );

    expect(useTerminal.getState().sessions).toEqual({});
    expect(useTerminal.getState().historyByScope).toEqual({});
    unmount();
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("close_terminal_pty", { sessionId: "pty-1" }),
    );
  });

  it("submits a one-shot provisioning command over the interactive PTY", async () => {
    const invoke = mockIpc({
      start_terminal_pty: () => "pty-provision",
      write_terminal_pty: () => undefined,
      resize_terminal_pty: () => undefined,
      close_terminal_pty: () => true,
    });
    render(
      <InteractiveTerminal serial="phone-1" initialCommand="hermes setup" onClose={vi.fn()} />,
    );

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("write_terminal_pty", {
        sessionId: "pty-provision",
        data: Array.from(new TextEncoder().encode("hermes setup\r")),
      }),
    );
    expect(useTerminal.getState().historyByScope).toEqual({});
  });

  it("copies a selection but leaves unselected Ctrl+C to the PTY", async () => {
    mockIpc({ start_terminal_pty: () => "pty-copy", close_terminal_pty: () => true });
    const clipboard = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { clipboard: { writeText: clipboard } });
    render(<InteractiveTerminal serial="phone-1" onClose={vi.fn()} />);
    await screen.findByText("Interactive SSH shell");
    const event = new KeyboardEvent("keydown", { key: "c", ctrlKey: true, cancelable: true });
    expect(terminalHarness.keyHandler?.(event)).toBe(true);
    expect(clipboard).not.toHaveBeenCalled();
    terminalHarness.selection = "selected output";
    expect(terminalHarness.keyHandler?.(event)).toBe(false);
    await waitFor(() => expect(clipboard).toHaveBeenCalledWith("selected output"));
    expect(event.defaultPrevented).toBe(true);
    vi.unstubAllGlobals();
  });
});
