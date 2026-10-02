import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useDevices } from "../../stores/devices";
import { useTerminal } from "../../stores/terminal";
import { device } from "../../test/fixtures";
import { mockIpc, resetStores } from "../../test/ipcMock";
import type { StreamEvent } from "../../types";
import { TerminalView } from "./TerminalView";

type Chan = { onmessage: (e: StreamEvent) => void };

describe("TerminalView", () => {
  beforeEach(() => {
    resetStores();
    useTerminal.setState({ sessions: {} });
    useDevices.getState().setDevices([device()]);
  });

  it("is labelled as the Android shell, not Termux", () => {
    mockIpc({});
    render(<TerminalView />);
    expect(screen.getByText(/runs as the shell user, not Termux/)).toBeInTheDocument();
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

  it("cancels a running command", async () => {
    const fn = mockIpc({ stream_command: () => "cmd-7", cancel_stream: () => true });
    render(<TerminalView />);
    await userEvent.type(screen.getByLabelText("Command"), "logcat{Enter}");
    await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(fn).toHaveBeenCalledWith("cancel_stream", { streamId: "cmd-7" });
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
});
