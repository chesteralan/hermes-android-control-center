import { create } from "zustand";
import { ipc } from "../lib/ipc";
import type { HermesChatEvent, HermesSessionMessage, HermesSessionSummary } from "../types";

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "tool";
  content: string;
}

export interface ChatEntry {
  sessionId: string | null;
  messages: ChatMessage[];
  streamId: string | null;
  requestId: string | null;
  running: boolean;
  stopping: boolean;
  error: string | null;
  sessions: HermesSessionSummary[];
  loadingSessions: boolean;
  loadingMessages: boolean;
  hasMoreMessages: boolean;
  messageOffset: number;
  sessionsError: string | null;
}

const EMPTY_CHAT: ChatEntry = {
  sessionId: null,
  messages: [],
  streamId: null,
  requestId: null,
  running: false,
  stopping: false,
  error: null,
  sessions: [],
  loadingSessions: false,
  loadingMessages: false,
  hasMoreMessages: false,
  messageOffset: 0,
  sessionsError: null,
};

interface ChatStore {
  byDevice: Record<string, ChatEntry>;
  send: (key: string, serial: string, prompt: string) => Promise<void>;
  refreshSessions: (key: string, serial: string) => Promise<void>;
  openSession: (key: string, serial: string, sessionId: string) => Promise<void>;
  loadMoreMessages: (key: string, serial: string) => Promise<void>;
  stop: (key: string) => Promise<void>;
  newConversation: (key: string) => Promise<void>;
}

function messageId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function errorText(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = String(error.message);
    const details = "details" in error && typeof error.details === "string" ? error.details : null;
    return details ? `${message}\n${details}` : message;
  }
  return String(error);
}

function toChatMessages(messages: HermesSessionMessage[]): ChatMessage[] {
  return messages.flatMap((message, index) => {
    const role = message.role.toLowerCase();
    if (role === "system") return [];
    if (role === "tool") {
      return [
        {
          id: message.id ?? `${index}-${messageId()}`,
          role: "tool" as const,
          content: `Tool ${message.toolName ?? "result"}${message.content ? `\n${message.content}` : ""}`,
        },
      ];
    }
    if (role !== "user" && role !== "assistant") return [];
    const items: ChatMessage[] = [];
    if (message.content) {
      items.push({
        id: message.id ?? `${index}-${messageId()}`,
        role,
        content: message.content,
      });
    }
    for (const toolName of [
      ...(message.toolCalls ?? []),
      ...(message.toolName ? [message.toolName] : []),
    ]) {
      items.push({
        id: `${message.id ?? index}-tool-${items.length}`,
        role: "tool",
        content: `Using ${toolName}…`,
      });
    }
    return items;
  });
}

export const useChat = create<ChatStore>((set, get) => {
  const patch = (key: string, update: Partial<ChatEntry>) =>
    set((state) => ({
      byDevice: {
        ...state.byDevice,
        [key]: { ...(state.byDevice[key] ?? EMPTY_CHAT), ...update },
      },
    }));

  return {
    byDevice: {},
    refreshSessions: async (key, serial) => {
      patch(key, { loadingSessions: true, sessionsError: null });
      try {
        const sessions = await ipc.listHermesSessions(serial);
        patch(key, { sessions, loadingSessions: false });
      } catch (error) {
        patch(key, {
          loadingSessions: false,
          sessionsError: errorText(error),
        });
      }
    },
    openSession: async (key, serial, sessionId) => {
      const current = get().byDevice[key] ?? EMPTY_CHAT;
      if (current.running) return;
      patch(key, {
        sessionId,
        messages: [],
        loadingMessages: true,
        hasMoreMessages: false,
        messageOffset: 0,
        error: null,
      });
      try {
        const page = await ipc.getHermesSessionMessages(serial, sessionId);
        const latest = get().byDevice[key] ?? EMPTY_CHAT;
        if (latest.sessionId !== sessionId) return;
        patch(key, {
          messages: toChatMessages(page.messages),
          loadingMessages: false,
          hasMoreMessages: page.hasMore,
          messageOffset: page.offset + page.messages.length,
        });
      } catch (error) {
        const latest = get().byDevice[key] ?? EMPTY_CHAT;
        if (latest.sessionId === sessionId) {
          patch(key, {
            loadingMessages: false,
            error: errorText(error),
          });
        }
      }
    },
    loadMoreMessages: async (key, serial) => {
      const current = get().byDevice[key] ?? EMPTY_CHAT;
      if (!current.sessionId || !current.hasMoreMessages || current.loadingMessages) return;
      const sessionId = current.sessionId;
      const offset = current.messageOffset;
      patch(key, { loadingMessages: true, error: null });
      try {
        const page = await ipc.getHermesSessionMessages(serial, sessionId, offset);
        const latest = get().byDevice[key] ?? EMPTY_CHAT;
        if (latest.sessionId !== sessionId) return;
        patch(key, {
          messages: [...latest.messages, ...toChatMessages(page.messages)],
          loadingMessages: false,
          hasMoreMessages: page.hasMore,
          messageOffset: page.offset + page.messages.length,
        });
      } catch (error) {
        const latest = get().byDevice[key] ?? EMPTY_CHAT;
        if (latest.sessionId === sessionId) {
          patch(key, {
            loadingMessages: false,
            error: errorText(error),
          });
        }
      }
    },
    send: async (key, serial, prompt) => {
      const text = prompt.trim();
      const current = get().byDevice[key] ?? EMPTY_CHAT;
      if (!text || current.running) return;

      const assistantId = messageId();
      patch(key, {
        messages: [
          ...current.messages,
          { id: messageId(), role: "user", content: text },
          { id: assistantId, role: "assistant", content: "" },
        ],
        requestId: assistantId,
        running: true,
        stopping: false,
        streamId: null,
        error: null,
      });

      let completed = false;
      const onEvent = (event: HermesChatEvent) => {
        const entry = get().byDevice[key] ?? EMPTY_CHAT;
        if (entry.requestId !== assistantId || entry.stopping) return;
        switch (event.type) {
          case "session":
            patch(key, { sessionId: event.sessionId });
            break;
          case "text":
            patch(key, {
              messages: entry.messages.map((message) =>
                message.id === assistantId
                  ? { ...message, content: message.content + event.text }
                  : message,
              ),
            });
            break;
          case "toolUse":
            patch(key, {
              messages: [
                ...entry.messages,
                { id: messageId(), role: "tool", content: `Using ${event.name}…` },
              ],
            });
            break;
          case "toolResult": {
            const summary = event.isError ? "failed" : "finished";
            const output = event.output?.trim();
            patch(key, {
              messages: [
                ...entry.messages,
                {
                  id: messageId(),
                  role: "tool",
                  content: `Tool ${event.name} ${summary}${output ? `\n${output}` : ""}`,
                },
              ],
            });
            break;
          }
          case "error":
            patch(key, { error: event.message });
            break;
          case "complete":
            completed = true;
            patch(key, {
              sessionId: event.sessionId ?? entry.sessionId,
              running: false,
              requestId: null,
              streamId: null,
              error:
                event.exitCode !== null && event.exitCode !== 0
                  ? (event.error ?? entry.error ?? `Hermes exited with code ${event.exitCode}.`)
                  : entry.error,
            });
            void get().refreshSessions(key, serial);
            break;
        }
      };

      try {
        const streamId = await ipc.startHermesChat(serial, text, current.sessionId, onEvent);
        const latest = get().byDevice[key] ?? EMPTY_CHAT;
        if (!completed && latest.requestId === assistantId && latest.stopping) {
          try {
            await ipc.cancelStream(streamId);
          } finally {
            const afterCancel = get().byDevice[key] ?? EMPTY_CHAT;
            if (afterCancel.requestId === assistantId) {
              patch(key, { running: false, stopping: false, requestId: null, streamId: null });
            }
          }
        } else if (!completed && latest.requestId === assistantId) {
          patch(key, { streamId });
        } else if (!completed) {
          await ipc.cancelStream(streamId).catch(() => false);
        }
      } catch (error) {
        if ((get().byDevice[key] ?? EMPTY_CHAT).requestId === assistantId) {
          patch(key, {
            running: false,
            stopping: false,
            requestId: null,
            streamId: null,
            error:
              typeof error === "object" && error !== null && "message" in error
                ? String(error.message)
                : String(error),
          });
        }
      }
    },
    stop: async (key) => {
      const entry = get().byDevice[key] ?? EMPTY_CHAT;
      if (!entry.running || entry.stopping) return;
      patch(key, { stopping: true });
      if (!entry.streamId) return;
      try {
        await ipc.cancelStream(entry.streamId);
      } finally {
        const current = get().byDevice[key] ?? EMPTY_CHAT;
        if (current.requestId === entry.requestId) {
          patch(key, {
            running: false,
            stopping: false,
            requestId: null,
            streamId: null,
          });
        }
      }
    },
    newConversation: async (key) => {
      await get().stop(key);
      patch(key, {
        sessionId: null,
        messages: [],
        streamId: null,
        requestId: null,
        running: false,
        stopping: false,
        error: null,
        loadingMessages: false,
        hasMoreMessages: false,
        messageOffset: 0,
      });
    },
  };
});

export { EMPTY_CHAT };
