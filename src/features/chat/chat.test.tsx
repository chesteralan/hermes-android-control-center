import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useDevices } from "../../stores/devices";
import { EMPTY_CHAT, useChat } from "../../stores/chat";
import { device } from "../../test/fixtures";
import { mockIpc, resetStores } from "../../test/ipcMock";
import { ChatView } from "./ChatView";

describe("ChatView", () => {
  beforeEach(() => {
    resetStores();
    useChat.setState({ byDevice: {} });
    useDevices.getState().setDevices([device()]);
  });

  it("streams replies and tool activity, resumes the session, and starts a new conversation", async () => {
    const requests: Array<Record<string, unknown>> = [];
    mockIpc({
      start_hermes_chat: (args) => {
        if (!args) throw new Error("chat args were not passed");
        requests.push(args);
        const channel = args.onEvent as { onmessage: (event: unknown) => void };
        const sessionId = requests.length === 1 ? "session-a" : "session-b";
        channel.onmessage({ type: "session", sessionId });
        channel.onmessage({ type: "text", text: "Hello from Hermes." });
        channel.onmessage({ type: "toolUse", name: "web_search" });
        channel.onmessage({
          type: "toolResult",
          name: "web_search",
          output: "Found useful pages.",
          isError: false,
        });
        channel.onmessage({ type: "complete", sessionId, exitCode: 0, error: null });
        return `chat-${requests.length}`;
      },
      cancel_stream: () => true,
    });

    render(<ChatView />);
    await userEvent.type(screen.getByRole("textbox", { name: "Message Hermes" }), "First prompt");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));

    expect(await screen.findByText("Hello from Hermes.")).toBeInTheDocument();
    expect(screen.getByText("Tool web_search finished")).toBeInTheDocument();
    expect(screen.getByText("Session session-a")).toBeInTheDocument();
    expect(requests[0]).toMatchObject({
      serial: device().serial,
      prompt: "First prompt",
      sessionId: null,
    });

    await userEvent.type(screen.getByRole("textbox", { name: "Message Hermes" }), "Follow-up");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(requests[1]).toMatchObject({ prompt: "Follow-up", sessionId: "session-a" });

    await userEvent.click(screen.getByRole("button", { name: "New chat" }));
    expect(screen.getByText("New conversation")).toBeInTheDocument();
    expect(useChat.getState().byDevice[device().deviceId ?? device().serial]).toEqual(EMPTY_CHAT);

    await userEvent.type(screen.getByRole("textbox", { name: "Message Hermes" }), "Fresh start");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(requests[2]).toMatchObject({ prompt: "Fresh start", sessionId: null });
  });

  it("cancels a stream that opens after the user already stopped it", async () => {
    let resolveStream: ((id: string) => void) | undefined;
    let lateChannel: { onmessage: (event: unknown) => void } | undefined;
    const fn = mockIpc({
      start_hermes_chat: (args) => {
        lateChannel = args?.onEvent as { onmessage: (event: unknown) => void };
        return new Promise<string>((resolve) => {
          resolveStream = resolve;
        });
      },
      cancel_stream: () => true,
    });

    render(<ChatView />);
    await userEvent.type(screen.getByRole("textbox", { name: "Message Hermes" }), "Slow start");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    await userEvent.click(await screen.findByRole("button", { name: "Stop response" }));

    expect(screen.getByText("Stopping Hermes…")).toBeInTheDocument();
    resolveStream?.("late-chat");
    await waitFor(() =>
      expect(fn).toHaveBeenCalledWith("cancel_stream", { streamId: "late-chat" }),
    );

    lateChannel?.onmessage({ type: "session", sessionId: "stale-session" });
    lateChannel?.onmessage({ type: "text", text: "Stale response" });
    expect(await screen.findByText("New conversation")).toBeInTheDocument();
    expect(screen.queryByText("Session stale-session")).not.toBeInTheDocument();
    expect(screen.queryByText("Stale response")).not.toBeInTheDocument();
  });
});
