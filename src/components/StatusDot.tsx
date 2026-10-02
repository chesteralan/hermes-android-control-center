export type Tone = "success" | "warning" | "danger" | "muted" | "accent";

const color: Record<Tone, string> = {
  success: "bg-success",
  warning: "bg-warning",
  danger: "bg-danger",
  muted: "bg-muted",
  accent: "bg-accent",
};

/** Status is always shown as dot + text, never color alone. */
export function StatusDot({ tone, label, pulse }: { tone: Tone; label: string; pulse?: boolean }) {
  return (
    <span className="inline-flex items-center gap-2" role="status">
      <span
        className={`inline-block h-2 w-2 rounded-full ${color[tone]} ${pulse ? "animate-pulse" : ""}`}
        aria-hidden
      />
      <span>{label}</span>
    </span>
  );
}
