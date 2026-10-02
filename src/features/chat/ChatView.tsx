import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { Button } from "../../components/Button";
import { useActiveDevice, deviceKey } from "../../stores/devices";
import { EMPTY_CHAT, useChat } from "../../stores/chat";
import { NoDeviceState } from "../device/NoDeviceState";
import { deviceTitle } from "../device/deviceStatus";

function sessionDate(value: string | null): string {
  if (!value) return "";
  const numeric = /^\d+$/.test(value) ? Number(value) : null;
  const date = new Date(numeric === null ? value : numeric < 1e12 ? numeric * 1000 : numeric);
  return Number.isNaN(date.getTime())
    ? value
    : date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
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
  const key = device ? deviceKey(device) : "";
  const serial = device?.serial;
  const entry = useChat((state) => state.byDevice[key] ?? EMPTY_CHAT);
  const send = useChat((state) => state.send);
  const refreshSessions = useChat((state) => state.refreshSessions);
  const openSession = useChat((state) => state.openSession);
  const loadMoreMessages = useChat((state) => state.loadMoreMessages);
  const stop = useChat((state) => state.stop);
  const newConversation = useChat((state) => state.newConversation);
  const [prompt, setPrompt] = useState("");
  const [sessionFilter, setSessionFilter] = useState("");
  const bottom = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (serial) void refreshSessions(key, serial);
  }, [serial, key, refreshSessions]);

  useEffect(() => {
    bottom.current?.scrollIntoView?.({ block: "end", behavior: "smooth" });
  }, [entry.messages]);

  if (!device) return <NoDeviceState />;

  const visibleSessions = entry.sessions.filter((session) => {
    const filter = sessionFilter.trim().toLowerCase();
    if (!filter) return true;
    return `${session.title ?? ""} ${session.preview ?? ""} ${session.source ?? ""}`
      .toLowerCase()
      .includes(filter);
  });

  function submit(event: FormEvent) {
    event.preventDefault();
    const text = prompt.trim();
    if (!text || entry.running || entry.loadingMessages || !device) return;
    setPrompt("");
    void send(key, device.serial, text);
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
            onClick={() => void newConversation(key)}
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
          <label className="sr-only" htmlFor="session-filter">
            Filter recent sessions
          </label>
          <input
            id="session-filter"
            value={sessionFilter}
            onChange={(event) => setSessionFilter(event.target.value)}
            placeholder="Filter recent sessions"
            className="mx-2 my-2 rounded border border-border bg-surface px-2 py-1.5 text-[12px] outline-none placeholder:text-muted"
          />
          <div className="min-h-0 flex-1 overflow-auto px-1 pb-2">
            {entry.sessionsError && (
              <p className="m-2 text-[11px] text-danger" role="alert">
                {entry.sessionsError}
              </p>
            )}
            {!entry.loadingSessions && !entry.sessionsError && entry.sessions.length === 0 && (
              <p className="m-2 text-[11px] text-muted">No sessions found.</p>
            )}
            {visibleSessions.map((session) => (
              <button
                key={session.sessionId}
                type="button"
                aria-current={entry.sessionId === session.sessionId ? "true" : undefined}
                onClick={() => void openSession(key, device.serial, session.sessionId)}
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
            className="flex-1 space-y-4 overflow-auto px-4 py-5"
            role="log"
            aria-label="Chat messages"
          >
            {!entry.messages.length ? (
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
            <div ref={bottom} />
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
                      ? "Wait for Hermes to finish…"
                      : "Message Hermes"
                }
                disabled={entry.running || entry.loadingMessages}
                rows={2}
                className="max-h-40 min-h-12 flex-1 resize-y rounded-md border border-border bg-surface px-3 py-2 text-[13px] leading-5 outline-none placeholder:text-muted disabled:opacity-60"
              />
              <Button
                variant="primary"
                type="submit"
                disabled={entry.running || entry.loadingMessages || !prompt.trim()}
              >
                Send
              </Button>
            </div>
          </form>
        </div>
      </div>
    </section>
  );
}
