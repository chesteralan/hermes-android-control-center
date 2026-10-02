import { create } from "zustand";
import { ipc } from "../lib/ipc";
import type { ErrorPayload, StreamEvent } from "../types";

export const MAX_TERMINAL_LINES = 10_000;

export interface OutputLine {
  stream: "out" | "err";
  text: string;
}

export interface CommandBlock {
  id: number;
  command: string;
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

interface TerminalStore {
  /** Keyed by device key so each phone keeps its own scrollback. */
  sessions: Record<string, Session>;
  run: (deviceKey: string, serial: string, command: string) => Promise<void>;
  cancel: (deviceKey: string, blockId: number) => Promise<void>;
  clear: (deviceKey: string) => void;
}

let nextId = 1;

function trim(blocks: CommandBlock[]): CommandBlock[] {
  let total = blocks.reduce((n, b) => n + b.lines.length, 0);
  const out = [...blocks];
  while (total > MAX_TERMINAL_LINES && out.length > 1) {
    total -= out[0]?.lines.length ?? 0;
    out.shift();
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
          [key]: { blocks: trim(session.blocks.map((b) => (b.id === id ? fn(b) : b))) },
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

    run: async (key, serial, command) => {
      const id = nextId++;
      const block: CommandBlock = {
        id,
        command,
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
          [key]: { blocks: trim([...(s.sessions[key]?.blocks ?? []), block]) },
        },
      }));
      try {
        const streamId = await ipc.streamCommand(serial, command, (e) =>
          update(key, id, (b) => apply(b, e)),
        );
        update(key, id, (b) => (b.running ? { ...b, streamId } : b));
      } catch (e) {
        update(key, id, (b) => ({ ...b, running: false, error: e as ErrorPayload }));
      }
    },

    cancel: async (key, id) => {
      const block = get().sessions[key]?.blocks.find((b) => b.id === id);
      if (!block?.streamId) return;
      await ipc.cancelStream(block.streamId);
      update(key, id, (b) => ({ ...b, running: false, cancelled: true, streamId: null }));
    },

    clear: (key) => set((s) => ({ sessions: { ...s.sessions, [key]: { blocks: [] } } })),
  };
});
