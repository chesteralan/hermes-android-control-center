import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { deviceKey, useDevices } from "../../stores/devices";
import { chatSessionPreferenceKey, EMPTY_CHAT, useChat } from "../../stores/chat";
import { useSettings } from "../../stores/settings";
import { device } from "../../test/fixtures";
import { mockIpc, resetStores } from "../../test/ipcMock";
import type { AppConfig } from "../../types";
import { ChatView } from "./ChatView";

const testConfig: AppConfig = {
  version: 1,
  adbPath: null,
  knownAddresses: [],
  reconnect: { enabled: true, scheduleMs: [1000, 2000] },
  hermes: {
    environment: { type: "prootDistro", distro: "debian" },
    startMode: "supervised",
    transport: "termuxSsh",
    fallbackToSsh: false,
    gatewayCommand: "hermes gateway run",
    processMatch: "hermes-agent/venv/bin/python",
    gatewayMatch: "gateway run",
    hermesHome: "/root/.hermes",
    logFiles: ["/root/.hermes/logs/gateway.log", "/root/.hermes/logs/tool_calls.log"],
    pathPrepend: ["/root/.local/bin"],
    startCommand: "",
    stopCommand: "",
    restartCommand: "",
    statusCommand: "",
    logCommand: "",
    versionCommand: "hermes --version",
    doctorCommand: "hermes doctor",
    updateCommand: "hermes update",
  },
  logs: { autoStart: false, logcatFilter: "*:I" },
  termux: { sshUser: "termux", sshPort: 8022 },
  apiPort: 8765,
  logLevel: "info",
};

describe("ChatView", () => {
  beforeEach(() => {
    localStorage.clear();
    resetStores();
    useChat.setState({ byDevice: {} });
    useDevices.getState().setDevices([device()]);
    useSettings.setState({ saved: testConfig, draft: testConfig });
  });

  it("streams replies and tool activity, resumes the session, and starts a new conversation", async () => {
    const requests: Array<Record<string, unknown>> = [];
    mockIpc({
      list_hermes_sessions: () => [],
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
    const preferenceKey = chatSessionPreferenceKey(
      deviceKey(device()),
      testConfig.hermes.environment,
      testConfig.hermes.hermesHome,
    );
    expect(localStorage.getItem(preferenceKey)).toBe("session-b");

    await userEvent.click(screen.getByRole("button", { name: "New chat" }));
    expect(screen.getByText("New conversation")).toBeInTheDocument();
    expect(localStorage.getItem(preferenceKey)).toBeNull();
    expect(useChat.getState().byDevice[preferenceKey]).toEqual(EMPTY_CHAT);

    await userEvent.type(screen.getByRole("textbox", { name: "Message Hermes" }), "Fresh start");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(requests[2]).toMatchObject({ prompt: "Fresh start", sessionId: null });
  });

  it("opens a previous session, loads another page, and resumes that session", async () => {
    const session = {
      sessionId: "session-old",
      title: "Previous session",
      source: "telegram",
      model: "model-x",
      messageCount: 5,
      lastActive: "2026-10-02T12:00:00Z",
      preview: "A saved preview",
    };
    const messagePages = [
      {
        sessionId: "session-old",
        offset: 0,
        limit: 3,
        total: 5,
        hasMore: true,
        messages: [
          { id: "m1", role: "user", content: "Old question", timestamp: null, toolName: null },
          {
            id: "sys",
            role: "system",
            content: "Hidden system prompt",
            timestamp: null,
            toolName: null,
          },
          { id: "m2", role: "assistant", content: "Old answer", timestamp: null, toolName: null },
        ],
      },
      {
        sessionId: "session-old",
        offset: 3,
        limit: 500,
        total: 5,
        hasMore: false,
        messages: [
          { id: "m3", role: "user", content: "Second question", timestamp: null, toolName: null },
          {
            id: "m4",
            role: "assistant",
            content: "Second answer",
            timestamp: null,
            toolName: null,
          },
        ],
      },
    ];
    const requests: Array<Record<string, unknown>> = [];
    mockIpc({
      list_hermes_sessions: () => [session],
      get_hermes_session_messages: (args) => {
        if (!args) throw new Error("session args were not passed");
        requests.push(args);
        return messagePages[requests.length - 1];
      },
      start_hermes_chat: (args) => {
        if (!args) throw new Error("chat args were not passed");
        requests.push(args);
        const channel = args.onEvent as { onmessage: (event: unknown) => void };
        channel.onmessage({ type: "session", sessionId: "session-old" });
        channel.onmessage({ type: "text", text: "Continued answer" });
        channel.onmessage({ type: "complete", sessionId: "session-old", exitCode: 0, error: null });
        return "continued-stream";
      },
    });

    render(<ChatView />);
    const sessionButton = await screen.findByRole("button", { name: /Previous session/ });
    expect(screen.getByText("A saved preview")).toBeInTheDocument();
    await userEvent.click(sessionButton);

    expect(await screen.findByText("Old question")).toBeInTheDocument();
    expect(screen.getByText("Old answer")).toBeInTheDocument();
    expect(screen.queryByText("Hidden system prompt")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Load more messages" }));
    expect(await screen.findByText("Second answer")).toBeInTheDocument();
    expect(requests[1]).toMatchObject({ sessionId: "session-old", offset: 3 });

    await userEvent.type(screen.getByRole("textbox", { name: "Message Hermes" }), "Continue here");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(requests[2]).toMatchObject({ prompt: "Continue here", sessionId: "session-old" });
    expect(await screen.findByText("Continued answer")).toBeInTheDocument();
  });

  it("queues prompts while Hermes is running and starts them in order when available", async () => {
    const requests: Array<Record<string, unknown>> = [];
    const channels: Array<{ onmessage: (event: unknown) => void }> = [];
    mockIpc({
      list_hermes_sessions: () => [],
      start_hermes_chat: (args) => {
        if (!args) throw new Error("chat args were not passed");
        requests.push(args);
        channels.push(args.onEvent as { onmessage: (event: unknown) => void });
        return `queue-stream-${requests.length}`;
      },
    });

    render(<ChatView />);
    const composer = screen.getByRole("textbox", { name: "Message Hermes" });
    await userEvent.type(composer, "First prompt");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(requests).toHaveLength(1));

    expect(composer).toBeEnabled();
    await userEvent.type(composer, "Second prompt");
    await userEvent.click(screen.getByRole("button", { name: "Queue message" }));
    await userEvent.type(composer, "Third prompt");
    await userEvent.click(screen.getByRole("button", { name: "Queue message" }));
    expect(screen.getByRole("region", { name: "Queued messages" })).toHaveTextContent(
      "Up next · 2",
    );
    expect(requests).toHaveLength(1);

    channels[0]?.onmessage({ type: "session", sessionId: "session-first" });
    channels[0]?.onmessage({
      type: "complete",
      sessionId: "session-first",
      exitCode: 0,
      error: null,
    });
    await waitFor(() => expect(requests).toHaveLength(2));
    expect(requests[1]).toMatchObject({ prompt: "Second prompt", sessionId: "session-first" });

    channels[1]?.onmessage({ type: "session", sessionId: "session-second" });
    channels[1]?.onmessage({
      type: "complete",
      sessionId: "session-second",
      exitCode: 0,
      error: null,
    });
    await waitFor(() => expect(requests).toHaveLength(3));
    expect(requests[2]).toMatchObject({ prompt: "Third prompt", sessionId: "session-second" });

    channels[2]?.onmessage({
      type: "complete",
      sessionId: "session-second",
      exitCode: 0,
      error: null,
    });
    await waitFor(() =>
      expect(screen.queryByRole("region", { name: "Queued messages" })).toBeNull(),
    );
  });

  it("removes a queued prompt without sending it", async () => {
    const requests: Array<Record<string, unknown>> = [];
    mockIpc({
      list_hermes_sessions: () => [],
      start_hermes_chat: (args) => {
        if (!args) throw new Error("chat args were not passed");
        requests.push(args);
        return "active-stream";
      },
    });

    render(<ChatView />);
    const composer = screen.getByRole("textbox", { name: "Message Hermes" });
    await userEvent.type(composer, "Active prompt");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    await userEvent.type(composer, "Do not send this");
    await userEvent.click(screen.getByRole("button", { name: "Queue message" }));
    await userEvent.click(screen.getByRole("button", { name: "Remove queued message 1" }));

    expect(screen.queryByRole("region", { name: "Queued messages" })).toBeNull();
    expect(requests).toHaveLength(1);
  });

  it("starts the next queued prompt after the active response is stopped", async () => {
    const requests: Array<Record<string, unknown>> = [];
    const fn = mockIpc({
      list_hermes_sessions: () => [],
      start_hermes_chat: (args) => {
        if (!args) throw new Error("chat args were not passed");
        requests.push(args);
        return `stream-${requests.length}`;
      },
      cancel_stream: () => true,
    });

    render(<ChatView />);
    const composer = screen.getByRole("textbox", { name: "Message Hermes" });
    await userEvent.type(composer, "Long response");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    await screen.findByRole("button", { name: "Stop response" });
    await userEvent.type(composer, "Next response");
    await userEvent.click(screen.getByRole("button", { name: "Queue message" }));

    await userEvent.click(screen.getByRole("button", { name: "Stop response" }));
    await waitFor(() => expect(requests).toHaveLength(2));

    expect(fn).toHaveBeenCalledWith("cancel_stream", { streamId: "stream-1" });
    expect(requests[1]).toMatchObject({ prompt: "Next response", sessionId: null });
  });

  it("filters loaded sessions by source and text", async () => {
    mockIpc({
      list_hermes_sessions: () => [
        {
          sessionId: "telegram-session",
          title: "Telegram launch notes",
          source: "telegram",
          model: null,
          messageCount: 2,
          lastActive: null,
          preview: "Launch checklist",
        },
        {
          sessionId: "app-session",
          title: "Desktop preferences",
          source: "app",
          model: null,
          messageCount: 1,
          lastActive: null,
          preview: "Local settings",
        },
      ],
    });

    render(<ChatView />);
    expect(
      await screen.findByRole("button", { name: /Telegram launch notes/ }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Desktop preferences/ })).toBeInTheDocument();
    expect(
      screen.getByText("Filters apply only to the loaded recent sessions."),
    ).toBeInTheDocument();

    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Filter sessions by source" }),
      "telegram",
    );
    expect(screen.getByRole("button", { name: /Telegram launch notes/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Desktop preferences/ })).not.toBeInTheDocument();

    await userEvent.type(screen.getByRole("textbox", { name: "Filter recent sessions" }), "launch");
    expect(screen.getByRole("button", { name: /Telegram launch notes/ })).toBeInTheDocument();
    await userEvent.clear(screen.getByRole("textbox", { name: "Filter recent sessions" }));
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Filter sessions by source" }),
      "app",
    );
    expect(screen.getByRole("button", { name: /Desktop preferences/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Telegram launch notes/ })).not.toBeInTheDocument();
  });

  it("clears transient filters when the active device changes", async () => {
    const secondDevice = device({
      serial: "second-device-serial",
      deviceId: "SECONDDEVICE0002",
    });
    mockIpc({
      list_hermes_sessions: (args) =>
        args?.serial === device().serial
          ? [
              {
                sessionId: "telegram-session",
                title: "Telegram launch notes",
                source: "telegram",
                model: null,
                messageCount: 1,
                lastActive: null,
                preview: null,
              },
            ]
          : [
              {
                sessionId: "app-session",
                title: "App-only notes",
                source: "app",
                model: null,
                messageCount: 1,
                lastActive: null,
                preview: null,
              },
            ],
    });

    render(<ChatView />);
    await screen.findByRole("button", { name: /Telegram launch notes/ });
    const sourceFilter = screen.getByRole("combobox", { name: "Filter sessions by source" });
    const textFilter = screen.getByRole("textbox", { name: "Filter recent sessions" });
    await userEvent.selectOptions(sourceFilter, "telegram");
    await userEvent.type(textFilter, "launch");

    useDevices.getState().setDevices([device(), secondDevice]);
    useDevices.getState().select(deviceKey(secondDevice));

    expect(await screen.findByRole("button", { name: /App-only notes/ })).toBeInTheDocument();
    expect(sourceFilter).toHaveValue("");
    expect(textFilter).toHaveValue("");
  });

  it("keeps the selected session when refresh changes list order", async () => {
    const preferenceKey = chatSessionPreferenceKey(
      deviceKey(device()),
      testConfig.hermes.environment,
      testConfig.hermes.hermesHome,
    );
    useChat.setState({
      byDevice: { [preferenceKey]: { ...EMPTY_CHAT, sessionId: "selected-session" } },
    });
    let refreshCount = 0;
    const selected = {
      sessionId: "selected-session",
      title: "Selected conversation",
      source: "app",
      model: null,
      messageCount: 1,
      lastActive: null,
      preview: null,
    };
    const other = {
      sessionId: "other-session",
      title: "Other conversation",
      source: "app",
      model: null,
      messageCount: 1,
      lastActive: null,
      preview: null,
    };
    mockIpc({
      list_hermes_sessions: () => {
        refreshCount += 1;
        return refreshCount === 1 ? [selected, other] : [other, selected];
      },
    });

    render(<ChatView />);
    const selectedButton = await screen.findByRole("button", { name: /Selected conversation/ });
    expect(selectedButton).toHaveAttribute("aria-current", "true");
    await userEvent.click(screen.getByRole("button", { name: "Refresh sessions" }));
    await waitFor(() =>
      expect(useChat.getState().byDevice[preferenceKey]?.sessions[0]?.sessionId).toBe(
        "other-session",
      ),
    );
    expect(screen.getByRole("button", { name: /Selected conversation/ })).toHaveAttribute(
      "aria-current",
      "true",
    );
  });

  it("shows a session API error instead of an empty-list message", async () => {
    mockIpc({
      list_hermes_sessions: () => {
        throw new Error("Session service is unavailable.");
      },
    });

    render(<ChatView />);

    expect(await screen.findByRole("alert")).toHaveTextContent("Session service is unavailable.");
    expect(screen.queryByText("No sessions found.")).not.toBeInTheDocument();
  });

  it("restores only the selected session for the same device and Hermes environment/home", async () => {
    const preferenceKey = chatSessionPreferenceKey(
      deviceKey(device()),
      testConfig.hermes.environment,
      testConfig.hermes.hermesHome,
    );
    localStorage.setItem(preferenceKey, "session-old");
    const messageSerials: string[] = [];
    const handlers = {
      list_hermes_sessions: () => [
        {
          sessionId: "session-old",
          title: "Restored session",
          source: "app",
          model: null,
          messageCount: 1,
          lastActive: null,
          preview: "Preview must stay on the phone",
        },
      ],
      get_hermes_session_messages: (args: Record<string, unknown> | undefined) => {
        messageSerials.push(String(args?.serial));
        return {
          sessionId: "session-old",
          offset: 0,
          limit: 500,
          total: 1,
          hasMore: false,
          messages: [
            {
              id: "restored-message",
              role: "user",
              content: "Transcript must stay on the phone",
              timestamp: null,
              toolName: null,
              toolCalls: [],
            },
          ],
        };
      },
    };
    mockIpc(handlers);

    const firstView = render(<ChatView />);
    expect(await screen.findByText("Transcript must stay on the phone")).toBeInTheDocument();
    expect(localStorage.getItem(preferenceKey)).toBe("session-old");
    expect(localStorage.length).toBe(1);

    firstView.unmount();
    useChat.setState({ byDevice: {} });
    useDevices.getState().setDevices([device({ serial: "replacement-serial" })]);
    const reconnectedView = render(<ChatView />);
    expect(await screen.findByText("Transcript must stay on the phone")).toBeInTheDocument();
    expect(messageSerials).toEqual([device().serial, "replacement-serial"]);
    expect(localStorage.getItem(preferenceKey)).toBe("session-old");

    reconnectedView.unmount();
    useDevices.getState().setDevices([]);
    const offlineView = render(<ChatView />);
    expect(screen.getByText("No phone connected")).toBeInTheDocument();
    expect(localStorage.getItem(preferenceKey)).toBe("session-old");
    offlineView.unmount();

    useDevices.getState().setDevices([device({ serial: "replacement-serial" })]);
    const changedHome = {
      ...testConfig,
      hermes: { ...testConfig.hermes, hermesHome: "/root/another-hermes-home" },
    };
    useSettings.setState({ saved: changedHome, draft: changedHome });
    const otherHomeView = render(<ChatView />);
    expect(await screen.findByText("New conversation")).toBeInTheDocument();
    expect(messageSerials).toHaveLength(2);
    otherHomeView.unmount();

    const otherEnvironment = {
      ...testConfig,
      hermes: {
        ...testConfig.hermes,
        environment: { ...testConfig.hermes.environment, distro: "ubuntu" },
      },
    };
    useSettings.setState({ saved: otherEnvironment, draft: otherEnvironment });
    const otherEnvironmentView = render(<ChatView />);
    expect(await screen.findByText("New conversation")).toBeInTheDocument();
    expect(messageSerials).toHaveLength(2);
    expect(localStorage.getItem(preferenceKey)).toBe("session-old");
    otherEnvironmentView.unmount();
  });

  it("renders a measured viewport slice for long transcripts", async () => {
    const preferenceKey = chatSessionPreferenceKey(
      deviceKey(device()),
      testConfig.hermes.environment,
      testConfig.hermes.hermesHome,
    );
    const messages = Array.from({ length: 250 }, (_, index) => ({
      id: `message-${index}`,
      role: "user" as const,
      content: `Long history message ${index}`,
    }));
    useChat.setState({
      byDevice: {
        [preferenceKey]: { ...EMPTY_CHAT, sessionId: "long-session", messages },
      },
    });
    mockIpc({ list_hermes_sessions: () => [] });
    const offsetHeightSpy = vi
      .spyOn(HTMLElement.prototype, "offsetHeight", "get")
      .mockImplementation(function (this: HTMLElement) {
        return this.getAttribute("aria-label") === "Chat messages" ? 600 : 0;
      });

    try {
      render(<ChatView />);

      const renderedMessages = await screen.findAllByText(/Long history message/);
      expect(renderedMessages.length).toBeGreaterThan(0);
      expect(renderedMessages.length).toBeLessThan(messages.length);
    } finally {
      offsetHeightSpy.mockRestore();
    }
  });

  it("persists a session ID rotated by Hermes after compression", async () => {
    const preferenceKey = chatSessionPreferenceKey(
      deviceKey(device()),
      testConfig.hermes.environment,
      testConfig.hermes.hermesHome,
    );
    useChat.setState({
      byDevice: {
        [preferenceKey]: { ...EMPTY_CHAT, sessionId: "session-before-compression" },
      },
    });
    const requests: Array<Record<string, unknown>> = [];
    mockIpc({
      list_hermes_sessions: () => [],
      start_hermes_chat: (args) => {
        if (!args) throw new Error("chat args were not passed");
        requests.push(args);
        const channel = args.onEvent as { onmessage: (event: unknown) => void };
        channel.onmessage({ type: "session", sessionId: "session-after-compression" });
        channel.onmessage({ type: "text", text: "Continued after compression." });
        channel.onmessage({
          type: "complete",
          sessionId: "session-after-compression",
          exitCode: 0,
          error: null,
        });
        return "compressed-stream";
      },
    });

    render(<ChatView />);
    await userEvent.type(screen.getByRole("textbox", { name: "Message Hermes" }), "Continue");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));

    expect(requests[0]).toMatchObject({ sessionId: "session-before-compression" });
    expect(await screen.findByText("Session session-after-compression")).toBeInTheDocument();
    expect(localStorage.getItem(preferenceKey)).toBe("session-after-compression");
  });

  it("keeps the selected transcript and reports a failed resume", async () => {
    const session = {
      sessionId: "session-failed-resume",
      title: "Saved conversation",
      source: "app",
      model: null,
      messageCount: 1,
      lastActive: null,
      preview: null,
    };
    mockIpc({
      list_hermes_sessions: () => [session],
      get_hermes_session_messages: () => ({
        sessionId: session.sessionId,
        offset: 0,
        limit: 500,
        total: 1,
        hasMore: false,
        messages: [
          {
            id: "saved-question",
            role: "user",
            content: "Original question",
            timestamp: null,
            toolName: null,
            toolCalls: [],
          },
        ],
      }),
      start_hermes_chat: () => {
        throw new Error("Hermes could not resume this session.");
      },
    });

    render(<ChatView />);
    await userEvent.click(await screen.findByRole("button", { name: /Saved conversation/ }));
    expect(await screen.findByText("Original question")).toBeInTheDocument();
    await userEvent.type(screen.getByRole("textbox", { name: "Message Hermes" }), "Continue");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Hermes could not resume this session.",
    );
    expect(screen.getByText("Original question")).toBeInTheDocument();
    expect(screen.getByText(`Session ${session.sessionId}`)).toBeInTheDocument();
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
