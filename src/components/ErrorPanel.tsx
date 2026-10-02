import { useState } from "react";
import type { ErrorPayload } from "../types";

export function ErrorPanel({ error, onRetry }: { error: ErrorPayload; onRetry?: () => void }) {
  const [open, setOpen] = useState(false);
  return (
    <div role="alert" className="rounded-md border border-danger/40 bg-danger/10 p-3 text-[13px]">
      <p className="text-danger">{error.message}</p>
      <div className="mt-2 flex gap-3 text-muted">
        {error.details && (
          <button
            type="button"
            className="underline hover:text-text"
            onClick={() => setOpen((o) => !o)}
            aria-expanded={open}
          >
            {open ? "Hide details" : "Details"}
          </button>
        )}
        {onRetry && (
          <button type="button" className="underline hover:text-text" onClick={onRetry}>
            Retry
          </button>
        )}
      </div>
      {open && error.details && (
        <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap rounded bg-bg p-2 font-mono text-[12px] text-muted">
          {error.details}
        </pre>
      )}
    </div>
  );
}
