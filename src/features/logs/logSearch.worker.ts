export interface LogSearchWorkerLine {
  seq: number;
  text: string;
}

export interface LogSearchWorkerRequest {
  requestId: number;
  signature: string;
  query: string;
  regex: boolean;
  caseSensitive: boolean;
  lines: LogSearchWorkerLine[];
}

export interface LogSearchWorkerResponse {
  requestId: number;
  signature: string;
  matchedSeqs: number[];
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export function searchLogSequences(request: LogSearchWorkerRequest): LogSearchWorkerResponse {
  let pattern: RegExp;
  try {
    pattern = new RegExp(
      request.regex ? request.query : escapeRegExp(request.query),
      request.caseSensitive ? "" : "i",
    );
  } catch {
    return { requestId: request.requestId, signature: request.signature, matchedSeqs: [] };
  }
  return {
    requestId: request.requestId,
    signature: request.signature,
    matchedSeqs: request.lines.filter((line) => pattern.test(line.text)).map((line) => line.seq),
  };
}

interface WorkerScope {
  document?: unknown;
  onmessage: ((event: MessageEvent<LogSearchWorkerRequest>) => void) | null;
  postMessage: (message: LogSearchWorkerResponse) => void;
}

const workerScope = globalThis as unknown as WorkerScope;
if (typeof workerScope.document === "undefined" && typeof workerScope.postMessage === "function") {
  workerScope.onmessage = (event) => workerScope.postMessage(searchLogSequences(event.data));
}