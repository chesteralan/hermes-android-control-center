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
});
