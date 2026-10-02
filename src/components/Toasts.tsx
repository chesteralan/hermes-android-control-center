import { useToasts } from "../stores/toast";

const tone = {
  info: "border-border",
  success: "border-success/50",
  error: "border-danger/50",
} as const;

export function Toasts() {
  const { toasts, dismiss } = useToasts();
  return (
    <div className="fixed bottom-4 right-4 z-50 flex flex-col gap-2" aria-live="polite">
      {toasts.map((t) => (
        <button
          type="button"
          key={t.id}
          onClick={() => dismiss(t.id)}
          className={`rounded-md border bg-surface-2 px-3 py-2 text-left shadow-lg ${tone[t.tone]}`}
        >
          {t.text}
        </button>
      ))}
    </div>
  );
}
