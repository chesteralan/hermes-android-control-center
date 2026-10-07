import { create } from "zustand";
import { ipc } from "../lib/ipc";
import type { ErrorPayload, StreamEvent, TransportKind } from "../types";

export const MAX_TERMINAL_LINES = 10_000;
export const MAX_COMMAND_HISTORY = 500;
export const EMPTY_COMMAND_HISTORY: string[] = [];

const HISTORY_STORAGE_PREFIX = "hacc:terminal:history:";
const HISTORY_PERSISTENCE_KEY = `${HISTORY_STORAGE_PREFIX}enabled`;
const SNIPPETS_STORAGE_KEY = "hacc:terminal:snippets";
const commandTimeouts = new Map<string, ReturnType<typeof setTimeout>>();

function clearCommandTimeout(key: string): void {
  const timeout = commandTimeouts.get(key);
  if (timeout !== undefined) clearTimeout(timeout);
  commandTimeouts.delete(key);
}

export interface CommandSnippet {
  id: string;
  name: string;
  command: string;
  transport: TransportKind;
  requiresConfirmation: boolean;
}

export function terminalHistoryScope(deviceKey: string, transport: TransportKind): string {
  return JSON.stringify([deviceKey, transport]);
}

function commandHistoryStorageKey(scope: string): string {
  return `${HISTORY_STORAGE_PREFIX}${scope}`;
}

function readHistoryPersistence(): boolean {
  if (typeof localStorage === "undefined") return true;
  try {
    return localStorage.getItem(HISTORY_PERSISTENCE_KEY) !== "false";
  } catch {
    return true;
  }
}

function readStoredHistory(scope: string): string[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const parsed: unknown = JSON.parse(
      localStorage.getItem(commandHistoryStorageKey(scope)) ?? "[]",
    );
    return Array.isArray(parsed)
      ? parsed
          .filter((command): command is string => typeof command === "string")
          .slice(-MAX_COMMAND_HISTORY)
      : [];
  } catch {
    return [];
  }
}

function readStoredSnippets(): CommandSnippet[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(SNIPPETS_STORAGE_KEY) ?? "[]");
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (item): item is CommandSnippet =>
        typeof item === "object" &&
        item !== null &&
        "id" in item &&
        typeof item.id === "string" &&
        "name" in item &&
        typeof item.name === "string" &&
        "command" in item &&
        typeof item.command === "string" &&
        "transport" in item &&
        (item.transport === "adbShell" ||
          item.transport === "termuxSsh" ||
          item.transport === "api") &&
        "requiresConfirmation" in item &&
        typeof item.requiresConfirmation === "boolean",
    );
  } catch {
    return [];
  }
}

export interface OutputLine {
  stream: "out" | "err";
  text: string;
}

export interface CommandBlock {
  id: number;
  command: string;
  transport: TransportKind;
  lines: OutputLine[];
  running: boolean;
  streamId: string | null;
  exitCode: number | null;
  durationMs: number | null;
  cancelled: boolean;
  error: ErrorPayload | null;
}

interface Session {
  blocks: CommandBlock[];
}

export interface InteractiveLaunchRequest {
  serial: string;
  command: string;
}

export interface TerminalSessionTab {
  id: string;
  title: string;
}

export function terminalSessionKey(deviceKey: string, sessionId: string): string {
  return JSON.stringify([deviceKey, sessionId]);
}

interface TerminalStore {
  /** Session output is keyed by stable device key and tab ID. */
  sessions: Record<string, Session>;
  tabsByDevice: Record<string, TerminalSessionTab[]>;
  activeSessionByDevice: Record<string, string>;
  snippets: CommandSnippet[];
  historyByScope: Record<string, string[]>;
  persistedHistoryByScope: Record<string, string[]>;
  historyPersistenceEnabled: boolean;
  pendingInteractiveLaunch: InteractiveLaunchRequest | null;
  requestInteractiveLaunch: (serial: string, command: string) => void;
  consumeInteractiveLaunch: (serial: string) => string | null;
  run: (
    deviceKey: string,
    serial: string,
    command: string,
    transport?: TransportKind,
    sessionId?: string,
    timeoutMs?: number,
  ) => Promise<void>;
  cancel: (deviceKey: string, sessionId: string, blockId: number) => Promise<void>;
  clear: (deviceKey: string, sessionId: string) => void;
  createSession: (deviceKey: string) => string;
  ensureSession: (deviceKey: string) => string;
  selectSession: (deviceKey: string, sessionId: string) => void;
  closeSession: (deviceKey: string, sessionId: string) => Promise<void>;
  addSnippet: (snippet: Omit<CommandSnippet, "id">) => void;
  removeSnippet: (id: string) => void;
  loadCommandHistory: (scope: string) => void;
  recordCommand: (scope: string, command: string) => void;
  setHistoryPersistence: (enabled: boolean) => void;
}

let nextId = 1;
let nextSessionId = 1;

export function trimTerminalBlocks(blocks: CommandBlock[]): CommandBlock[] {
  let total = blocks.reduce((n, b) => n + b.lines.length, 0);
  const out = [...blocks];
  while (total > MAX_TERMINAL_LINES && out.length > 0) {
    const first = out[0];
    if (!first) break;
    const excess = total - MAX_TERMINAL_LINES;
    if (out.length > 1 && first.lines.length <= excess) {
      total -= first.lines.length;
      out.shift();
    } else {
      const lines = first.lines.slice(excess);
      total -= excess;
      out[0] = { ...first, lines };
    }
  }
  return out;
}

export const useTerminal = create<TerminalStore>((set, get) => {
  const update = (key: string, id: number, fn: (b: CommandBlock) => CommandBlock) =>
    set((s) => {
      const session = s.sessions[key];
      if (!session) return s;
      return {
        sessions: {
          ...s.sessions,
          [key]: {
            blocks: trimTerminalBlocks(session.blocks.map((b) => (b.id === id ? fn(b) : b))),
          },
        },
      };
    });

  const apply = (b: CommandBlock, e: StreamEvent): CommandBlock => {
    switch (e.type) {
      case "stdout":
        return { ...b, lines: [...b.lines, { stream: "out", text: e.line }] };
      case "stderr":
        return { ...b, lines: [...b.lines, { stream: "err", text: e.line }] };
      case "exit":
        return { ...b, running: false, exitCode: e.code, durationMs: e.durationMs, streamId: null };
      case "error":
        return { ...b, running: false, error: e.error, streamId: null };
    }
  };

  return {
    sessions: {},
    tabsByDevice: {},
    activeSessionByDevice: {},
    snippets: readStoredSnippets(),
    historyByScope: {},
    persistedHistoryByScope: {},
    historyPersistenceEnabled: readHistoryPersistence(),
    pendingInteractiveLaunch: null,
    requestInteractiveLaunch: (serial, command) =>
      set({ pendingInteractiveLaunch: { serial, command } }),
    consumeInteractiveLaunch: (serial) => {
      const pending = get().pendingInteractiveLaunch;
      if (!pending || pending.serial !== serial) return null;
      set({ pendingInteractiveLaunch: null });
      return pending.command;
    },

    loadCommandHistory: (scope) => {
      if (Object.hasOwn(get().historyByScope, scope)) return;
      const history = get().historyPersistenceEnabled ? readStoredHistory(scope) : [];
      set((state) => ({
        historyByScope: { ...state.historyByScope, [scope]: history },
        persistedHistoryByScope: state.historyPersistenceEnabled
          ? { ...state.persistedHistoryByScope, [scope]: history }
          : state.persistedHistoryByScope,
      }));
    },

    recordCommand: (scope, command) => {
      const normalized = command.trim();
      if (!normalized) return;
      get().loadCommandHistory(scope);
      const state = get();
      const history = state.historyByScope[scope] ?? EMPTY_COMMAND_HISTORY;
      const nextHistory =
        history[history.length - 1] === normalized
          ? history
          : [...history, normalized].slice(-MAX_COMMAND_HISTORY);
      let nextPersistedHistory = state.persistedHistoryByScope[scope] ?? EMPTY_COMMAND_HISTORY;
      if (state.historyPersistenceEnabled) {
        if (nextPersistedHistory[nextPersistedHistory.length - 1] !== normalized) {
          nextPersistedHistory = [...nextPersistedHistory, normalized].slice(-MAX_COMMAND_HISTORY);
        }
        try {
          localStorage.setItem(
            commandHistoryStorageKey(scope),
            JSON.stringify(nextPersistedHistory),
          );
        } catch {
          nextPersistedHistory = state.persistedHistoryByScope[scope] ?? EMPTY_COMMAND_HISTORY;
        }
      }
      set((current) => ({
        historyByScope: { ...current.historyByScope, [scope]: nextHistory },
        persistedHistoryByScope: state.historyPersistenceEnabled
          ? { ...current.persistedHistoryByScope, [scope]: nextPersistedHistory }
          : current.persistedHistoryByScope,
      }));
    },

    setHistoryPersistence: (enabled) => {
      try {
        if (typeof localStorage !== "undefined") {
          if (enabled) {
            localStorage.setItem(HISTORY_PERSISTENCE_KEY, "true");
          } else {
            for (let index = localStorage.length - 1; index >= 0; index -= 1) {
              const storageKey = localStorage.key(index);
              if (storageKey?.startsWith(HISTORY_STORAGE_PREFIX)) {
                localStorage.removeItem(storageKey);
              }
            }
            localStorage.setItem(HISTORY_PERSISTENCE_KEY, "false");
          }
        }
      } catch {
        set({ historyPersistenceEnabled: enabled });
        return;
      }
      set((state) => ({
        historyPersistenceEnabled: enabled,
        persistedHistoryByScope: enabled ? state.persistedHistoryByScope : {},
      }));
    },

    addSnippet: (snippet) => {
      const name = snippet.name.trim();
      const command = snippet.command.trim();
      if (!name || !command) return;
      const next = [
        ...get().snippets,
        {
          ...snippet,
          id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          name,
          command,
        },
      ];
      try {
        localStorage.setItem(SNIPPETS_STORAGE_KEY, JSON.stringify(next));
      } catch {
        set({ snippets: next });
        return;
      }
      set({ snippets: next });
    },

    removeSnippet: (id) => {
      const next = get().snippets.filter((snippet) => snippet.id !== id);
      try {
        localStorage.setItem(SNIPPETS_STORAGE_KEY, JSON.stringify(next));
      } catch {
        set({ snippets: next });
        return;
      }
      set({ snippets: next });
    },

    createSession: (key) => {
      const id = `terminal-${nextSessionId++}`;
      const tabs = get().tabsByDevice[key] ?? [];
      set((state) => ({
        sessions: { ...state.sessions, [terminalSessionKey(key, id)]: { blocks: [] } },
        tabsByDevice: {
          ...state.tabsByDevice,
          [key]: [...tabs, { id, title: `Terminal ${tabs.length + 1}` }],
        },
        activeSessionByDevice: { ...state.activeSessionByDevice, [key]: id },
      }));
      return id;
    },

    ensureSession: (key) => {
      const activeId = get().activeSessionByDevice[key];
      if (activeId && get().tabsByDevice[key]?.some((tab) => tab.id === activeId)) return activeId;
      return get().createSession(key);
    },

    selectSession: (key, sessionId) => {
      if (!get().tabsByDevice[key]?.some((tab) => tab.id === sessionId)) return;
      set((state) => ({
        activeSessionByDevice: { ...state.activeSessionByDevice, [key]: sessionId },
      }));
    },

    closeSession: async (key, sessionId) => {
      const tabs = get().tabsByDevice[key] ?? [];
      if (tabs.length < 2 || !tabs.some((tab) => tab.id === sessionId)) return;
      const sessionKey = terminalSessionKey(key, sessionId);
      const session = get().sessions[sessionKey];
      for (const block of session?.blocks ?? []) {
        if (block.running) await get().cancel(key, sessionId, block.id);
      }
      set((state) => {
        const remaining = (state.tabsByDevice[key] ?? []).filter((tab) => tab.id !== sessionId);
        const sessions = { ...state.sessions };
        delete sessions[sessionKey];
        const activeId = state.activeSessionByDevice[key];
        const closedIndex = tabs.findIndex((tab) => tab.id === sessionId);
        const fallback = remaining[Math.max(0, closedIndex - 1)] ?? remaining[0];
        return {
          sessions,
          tabsByDevice: { ...state.tabsByDevice, [key]: remaining },
          activeSessionByDevice:
            activeId === sessionId && fallback
              ? { ...state.activeSessionByDevice, [key]: fallback.id }
              : state.activeSessionByDevice,
        };
      });
    },

    run: async (
      key,
      serial,
      command,
      transport = "adbShell",
      requestedSessionId,
      timeoutMs = 0,
    ) => {
      const sessionId = requestedSessionId ?? get().ensureSession(key);
      const sessionKey = terminalSessionKey(key, sessionId);
      const timeoutKey = `${sessionKey}:${nextId}`;
      get().recordCommand(terminalHistoryScope(key, transport), command);
      const id = nextId++;
      const block: CommandBlock = {
        id,
        command,
        transport,
        lines: [],
        running: true,
        streamId: null,
        exitCode: null,
        durationMs: null,
        cancelled: false,
        error: null,
      };
      set((s) => ({
        sessions: {
          ...s.sessions,
          [sessionKey]: {
            blocks: trimTerminalBlocks([...(s.sessions[sessionKey]?.blocks ?? []), block]),
          },
        },
      }));
      let pendingLines: OutputLine[] = [];
      let flushTimer: ReturnType<typeof setTimeout> | null = null;
      const flushLines = () => {
        if (flushTimer !== null) clearTimeout(flushTimer);
        flushTimer = null;
        if (pendingLines.length === 0) return;
        const batch = pendingLines;
        pendingLines = [];
        update(sessionKey, id, (block) => ({ ...block, lines: [...block.lines, ...batch] }));
      };
      const onEvent = (event: StreamEvent) => {
        if (event.type === "stdout" || event.type === "stderr") {
          pendingLines.push({ stream: event.type === "stdout" ? "out" : "err", text: event.line });
          if (flushTimer === null) flushTimer = setTimeout(flushLines, 16);
          return;
        }
        flushLines();
        if (event.type === "exit" || event.type === "error") {
          clearCommandTimeout(timeoutKey);
        }
        update(sessionKey, id, (block) => apply(block, event));
      };
      try {
        const streamId = await ipc.streamCommand(serial, command, transport, onEvent);
        const latestSession = get().sessions[sessionKey];
        const latestBlock = latestSession?.blocks.find((candidate) => candidate.id === id);
        if (!latestBlock) {
          clearCommandTimeout(timeoutKey);
          await ipc.cancelStream(streamId).catch(() => false);
          return;
        }
        if (!latestBlock.running) {
          if (latestBlock.cancelled) await ipc.cancelStream(streamId).catch(() => false);
          clearCommandTimeout(timeoutKey);
          return;
        }
        update(sessionKey, id, (b) => ({ ...b, streamId }));
        if (timeoutMs > 0) {
          commandTimeouts.set(
            timeoutKey,
            setTimeout(() => {
              commandTimeouts.delete(timeoutKey);
              void get().cancel(key, sessionId, id);
            }, timeoutMs),
          );
        }
      } catch (e) {
        clearCommandTimeout(timeoutKey);
        flushLines();
        update(sessionKey, id, (b) => ({ ...b, running: false, error: e as ErrorPayload }));
      }
    },

    cancel: async (key, sessionId, id) => {
      const sessionKey = terminalSessionKey(key, sessionId);
      clearCommandTimeout(`${sessionKey}:${id}`);
      const block = get().sessions[sessionKey]?.blocks.find((b) => b.id === id);
      if (!block?.running) return;
      if (block.streamId) await ipc.cancelStream(block.streamId);
      update(sessionKey, id, (b) => ({ ...b, running: false, cancelled: true, streamId: null }));
    },

    clear: (key, sessionId) => {
      const sessionKey = terminalSessionKey(key, sessionId);
      set((state) => ({
        sessions: { ...state.sessions, [sessionKey]: { blocks: [] } },
      }));
    },
  };
});
