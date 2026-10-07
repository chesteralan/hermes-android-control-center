import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { Button } from "../../components/Button";
import { useActiveDevice, deviceKey } from "../../stores/devices";
import { chatSessionPreferenceKey, EMPTY_CHAT, useChat } from "../../stores/chat";
import { useSettings } from "../../stores/settings";
import { NoDeviceState } from "../device/NoDeviceState";
import { deviceTitle } from "../device/deviceStatus";

const VIRTUAL_MESSAGE_THRESHOLD = 100;

function sessionDate(value: string | null): string {
  if (!value) return "";
  const numeric = /^\d+$/.test(value) ? Number(value) : null;
  const date = new Date(numeric === null ? value : numeric < 1e12 ? numeric * 1000 : numeric);
  return Number.isNaN(date.getTime())
    ? value
    : date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

function readSelectedSession(preferenceKey: string): string | null {
  if (typeof localStorage === "undefined") return null;
  try {
    return localStorage.getItem(preferenceKey) || null;
  } catch {
    return null;
  }
}

function Message({ role, content }: { role: "user" | "assistant" | "tool"; content: string }) {
  if (role === "tool") {
    return (
      <details className="ml-3 border-l-2 border-border pl-3 text-[12px] text-muted">
        <summary className="cursor-pointer whitespace-pre-wrap">
          {content.split("\n", 1)[0]}
        </summary>
        {content.includes("\n") && (
          <pre className="mt-2 max-h-56 overflow-auto whitespace-pre-wrap font-mono text-[11px]">
            {content.slice(content.indexOf("\n") + 1)}
          </pre>
        )}
      </details>
    );
  }
  return (
    <article className={`max-w-[min(85%,760px)] ${role === "user" ? "ml-auto" : "mr-auto"}`}>
      <p className="mb-1 text-[10px] font-semibold uppercase text-muted">
        {role === "user" ? "You" : "Hermes"}
      </p>
      <div
        className={`rounded-md border px-3 py-2.5 text-[13px] leading-6 ${
          role === "user" ? "border-accent/25 bg-accent/10" : "border-border bg-bg"
        }`}
      >
        <p className="whitespace-pre-wrap wrap-break-word">{content || " "}</p>
      </div>
    </article>
  );
}

export function ChatView() {
  const device = useActiveDevice();
  const hermesConfig = useSettings((state) => state.saved?.hermes ?? null);
  const settingsError = useSettings((state) => state.error);
  const loadSettings = useSettings((state) => state.load);
  const deviceIdentity = device ? deviceKey(device) : null;
  const preferenceKey =
    deviceIdentity && hermesConfig
      ? chatSessionPreferenceKey(
          deviceIdentity,
          hermesConfig.environment,
          hermesConfig.hermesHome,
        )
      : null;
  const key = preferenceKey ?? deviceIdentity ?? "";
  const serial = device?.serial;
  const entry = useChat((state) => state.byDevice[key] ?? EMPTY_CHAT);
  const send = useChat((state) => state.send);
  const refreshSessions = useChat((state) => state.refreshSessions);
  const openSession = useChat((state) => state.openSession);
  const loadMoreMessages = useChat((state) => state.loadMoreMessages);
  const stop = useChat((state) => state.stop);
  const removeQueuedPrompt = useChat((state) => state.removeQueuedPrompt);
  const newConversation = useChat((state) => state.newConversation);
  const [prompt, setPrompt] = useState("");
  const [sessionFilter, setSessionFilter] = useState("");
  const [sourceFilter, setSourceFilter] = useState("");
  const messagesScroller = useRef<HTMLDivElement>(null);
  // eslint-disable-next-line react-hooks/incompatible-library -- virtualizer state is read during render by design
  const messageVirtualizer = useVirtualizer({
    count: entry.messages.length,
    getScrollElement: () => messagesScroller.current,
    estimateSize: () => 120,
    initialRect: { width: 800, height: 600 },
    overscan: 8,
  });
  const bottom = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!hermesConfig && !settingsError) void loadSettings();
  }, [hermesConfig, settingsError, loadSettings]);

  useEffect(() => {
    setSessionFilter("");
    setSourceFilter("");
  }, [preferenceKey]);

  useEffect(() => {
    if (!serial || !preferenceKey) return;
    void refreshSessions(key, serial);
    const selectedSessionId = readSelectedSession(preferenceKey);
    if (selectedSessionId) {
      void openSession(key, serial, selectedSessionId, preferenceKey);
    }
  }, [serial, key, preferenceKey, refreshSessions, openSession]);

  useEffect(() => {
    if (entry.messages.length > VIRTUAL_MESSAGE_THRESHOLD) {
      messageVirtualizer.scrollToIndex(entry.messages.length - 1, { align: "end" });
    } else {
      bottom.current?.scrollIntoView?.({ block: "end", behavior: "smooth" });
    }
  }, [entry.messages, messageVirtualizer]);

  if (!device) return <NoDeviceState />;
  if (!hermesConfig || !preferenceKey) {
    return (
      <p className="text-muted" role={settingsError ? "alert" : "status"}>
        {settingsError?.message ?? "Loading Hermes settings…"}
      </p>
    );
  }

  const sourceOptions = Array.from(
    new Set(entry.sessions.map((session) => session.source).filter((source): source is string => !!source)),
  ).sort();
  const visibleSessions = entry.sessions.filter((session) => {
    const query = sessionFilter.trim().toLowerCase();
    const matchesQuery =
      !query ||
      `${session.title ?? ""} ${session.preview ?? ""} ${session.source ?? ""}`
        .toLowerCase()
        .includes(query);
    return matchesQuery && (!sourceFilter || session.source === sourceFilter);
  });

  function submit(event: FormEvent) {
    event.preventDefault();
    const text = prompt.trim();
    if (!text || entry.loadingMessages || !device || !preferenceKey) return;
    setPrompt("");
    void send(key, device.serial, text, preferenceKey);
  }

  function onComposerKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      event.currentTarget.form?.requestSubmit();
    }
  }

  return (
    <section className="flex h-[calc(100vh-44px-48px)] min-h-105 flex-col overflow-hidden rounded-lg border border-border bg-surface">
      <header className="flex min-h-12 items-center justify-between gap-3 border-b border-border px-4 py-2">
        <div className="min-w-0">
          <h1 className="font-semibold">Chat · {deviceTitle(device)}</h1>
          <p className="truncate text-[11px] text-muted">
            {entry.sessionId ? `Session ${entry.sessionId}` : "New conversation"}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {entry.running && (
            <Button variant="danger" loading={entry.stopping} onClick={() => void stop(key)}>
              {entry.stopping ? "Stopping" : "Stop response"}
            </Button>
          )}
          <Button
            onClick={() => void newConversation(key, preferenceKey)}
            disabled={
              (!entry.messages.length && !entry.sessionId) || entry.running || entry.loadingMessages
            }
          >
            New chat
          </Button>
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <aside className="flex w-64 shrink-0 flex-col border-r border-border bg-bg/40">
          <div className="flex items-center justify-between border-b border-border px-3 py-2">
            <h2 className="text-[11px] font-semibold uppercase text-muted">Sessions</h2>
            <Button
              variant="ghost"
              loading={entry.loadingSessions}
              aria-label="Refresh sessions"
              onClick={() => void refreshSessions(key, device.serial)}
            >
              Refresh
            </Button>
          </div>
          <div className="space-y-2 px-2 py-2">
            <label className="sr-only" htmlFor="session-source-filter">
              Filter sessions by source
            </label>
            <select
              id="session-source-filter"
              value={sourceFilter}
              onChange={(event) => setSourceFilter(event.target.value)}
              className="w-full rounded border border-border bg-surface px-2 py-1.5 text-[12px]"
            >
              <option value="">All sources</option>
              {sourceOptions.map((source) => (
                <option key={source} value={source}>
                  {source}
                </option>
              ))}
            </select>
            <label className="sr-only" htmlFor="session-filter">
              Filter recent sessions
            </label>
            <input
              id="session-filter"
              value={sessionFilter}
              onChange={(event) => setSessionFilter(event.target.value)}
              placeholder="Filter recent sessions"
              className="w-full rounded border border-border bg-surface px-2 py-1.5 text-[12px] outline-none placeholder:text-muted"
            />
          </div>
          <p className="px-3 pb-2 text-[10px] text-muted">
            Filters apply only to the loaded recent sessions.
          </p>
          <div className="min-h-0 flex-1 overflow-auto px-1 pb-2">
            {entry.sessionsError && (
              <p className="m-2 text-[11px] text-danger" role="alert">
                {entry.sessionsError}
              </p>
            )}
            {!entry.loadingSessions && !entry.sessionsError && entry.sessions.length === 0 && (
              <p className="m-2 text-[11px] text-muted">No sessions found.</p>
            )}
            {!entry.loadingSessions &&
              !entry.sessionsError &&
              entry.sessions.length > 0 &&
              visibleSessions.length === 0 && (
                <p className="m-2 text-[11px] text-muted" role="status">
                  No matching sessions.
                </p>
              )}
            {visibleSessions.map((session) => (
              <button
                key={session.sessionId}
                type="button"
                aria-current={entry.sessionId === session.sessionId ? "true" : undefined}
                onClick={() =>
                  void openSession(key, device.serial, session.sessionId, preferenceKey)
                }
                disabled={entry.running}
                className={`mb-1 w-full rounded px-2.5 py-2 text-left disabled:opacity-50 ${
                  entry.sessionId === session.sessionId
                    ? "bg-accent/10 text-text"
                    : "text-muted hover:bg-surface-2 hover:text-text"
                }`}
              >
                <span className="block truncate text-[12px] font-medium text-text">
                  {session.title || session.preview || "Untitled session"}
                </span>
                {session.title && session.preview && (
                  <span className="mt-1 block truncate text-[11px]">{session.preview}</span>
                )}
                <span className="mt-1 flex justify-between gap-2 text-[10px]">
                  <span className="truncate">{session.source ?? "Hermes"}</span>
                  <span className="shrink-0">
                    {sessionDate(session.lastActive)}
                    {session.messageCount == null ? "" : ` · ${session.messageCount}`}
                  </span>
                </span>
              </button>
            ))}
          </div>
        </aside>

        <div className="flex min-w-0 flex-1 flex-col">
          {entry.loadingMessages && (
            <p className="border-b border-border px-4 py-2 text-[11px] text-muted" role="status">
              Loading conversation…
            </p>
          )}
          <div
            ref={messagesScroller}
            className="flex-1 space-y-4 overflow-auto px-4 py-5"
            role="log"
            aria-label="Chat messages"
          >
            {entry.messages.length > VIRTUAL_MESSAGE_THRESHOLD ? (
              <div style={{ height: messageVirtualizer.getTotalSize(), position: "relative" }}>
                {messageVirtualizer.getVirtualItems().map((item) => {
                  const message = entry.messages[item.index];
                  return message ? (
                    <div
                      key={message.id}
                      data-index={item.index}
                      ref={messageVirtualizer.measureElement}
                      className="absolute left-0 right-0 top-0 pb-4"
                      style={{ transform: `translateY(${item.start}px)` }}
                    >
                      <Message {...message} />
                    </div>
                  ) : null;
                })}
              </div>
            ) : !entry.messages.length ? (
              <div className="flex h-full min-h-48 flex-col items-center justify-center text-center">
                <h2 className="text-lg font-semibold">Start a conversation</h2>
                <p className="mt-2 max-w-md text-muted">
                  Send a prompt to Hermes. Replies stream here, and follow-up prompts stay in the
                  same session.
                </p>
              </div>
            ) : (
              entry.messages.map((message) => <Message key={message.id} {...message} />)
            )}
            {entry.hasMoreMessages && (
              <div className="flex justify-center">
                <Button
                  variant="ghost"
                  loading={entry.loadingMessages}
                  onClick={() => void loadMoreMessages(key, device.serial)}
                >
                  Load more messages
                </Button>
              </div>
            )}
            {entry.running && (
              <p className="text-[12px] text-accent" role="status">
                {entry.stopping ? "Stopping Hermes…" : "Hermes is responding…"}
              </p>
            )}
            {entry.error && (
              <div
                role="alert"
                className="rounded-md border border-danger/40 bg-danger/5 px-3 py-2 text-danger"
              >
                {entry.error}
              </div>
            )}
            {entry.queuedPrompts.length > 0 && (
              <section
                aria-label="Queued messages"
                className="mx-auto w-full max-w-3xl rounded border border-border bg-bg/60 p-3"
              >
                <p className="mb-2 text-[11px] font-semibold text-muted">
                  Up next · {entry.queuedPrompts.length}
                </p>
                <ol className="space-y-2">
                  {entry.queuedPrompts.map((queuedPrompt, index) => (
                    <li key={queuedPrompt.id} className="flex items-start gap-2 text-[12px]">
                      <span className="shrink-0 text-muted">{index + 1}.</span>
                      <p className="min-w-0 flex-1 whitespace-pre-wrap wrap-break-word text-text">
                        {queuedPrompt.prompt}
                      </p>
                      <Button
                        variant="ghost"
                        aria-label={`Remove queued message ${index + 1}`}
                        onClick={() => removeQueuedPrompt(key, queuedPrompt.id)}
                      >
                        Remove
                      </Button>
                    </li>
                  ))}
                </ol>
              </section>
            )}
            {entry.messages.length <= VIRTUAL_MESSAGE_THRESHOLD && <div ref={bottom} />}
          </div>

          <form onSubmit={submit} className="border-t border-border bg-bg/60 p-3">
            <label className="sr-only" htmlFor="hermes-prompt">
              Message Hermes
            </label>
            <div className="flex items-end gap-2">
              <textarea
                id="hermes-prompt"
                value={prompt}
                onChange={(event) => setPrompt(event.target.value)}
                onKeyDown={onComposerKeyDown}
                placeholder={
                  entry.loadingMessages
                    ? "Loading conversation…"
                    : entry.running
                      ? "Hermes is responding · next message queues…"
                      : "Message Hermes"
                }
                disabled={entry.loadingMessages}
                rows={2}
                className="max-h-40 min-h-12 flex-1 resize-y rounded-md border border-border bg-surface px-3 py-2 text-[13px] leading-5 outline-none placeholder:text-muted disabled:opacity-60"
              />
              <Button
                variant="primary"
                type="submit"
                disabled={entry.loadingMessages || !prompt.trim()}
              >
                {entry.running ? "Queue message" : "Send"}
              </Button>
            </div>
          </form>
        </div>
      </div>
    </section>
  );
}
