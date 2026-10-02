/** True on macOS (⌘), false on Windows/Linux (Ctrl). */
export const isMac =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

export const modLabel = isMac ? "⌘" : "Ctrl+";

export function isModKey(e: KeyboardEvent): boolean {
  return isMac ? e.metaKey : e.ctrlKey;
}
