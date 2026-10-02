import type { ButtonHTMLAttributes } from "react";

type Variant = "default" | "primary" | "danger" | "ghost";

const styles: Record<Variant, string> = {
  default: "bg-surface-2 border-border hover:border-muted text-text",
  primary: "bg-accent/15 border-accent/50 hover:bg-accent/25 text-accent",
  danger: "bg-danger/10 border-danger/50 hover:bg-danger/20 text-danger",
  ghost: "bg-transparent border-transparent hover:bg-surface-2 text-muted hover:text-text",
};

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  loading?: boolean;
}

export function Button({
  variant = "default",
  loading,
  disabled,
  children,
  className = "",
  ...rest
}: Props) {
  return (
    <button
      type="button"
      {...rest}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      className={`inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-[13px] transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${styles[variant]} ${className}`}
    >
      {loading && (
        <span className="h-3 w-3 animate-spin rounded-full border-2 border-current border-t-transparent" />
      )}
      {children}
    </button>
  );
}
