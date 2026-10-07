export type DesktopPlatform = "macos" | "windows" | "linux";

export function modifierLabel(platform: DesktopPlatform): string {
  return platform === "macos" ? "⌘" : "Ctrl+";
}

export function monospaceFontFor(platform: DesktopPlatform): string {
  if (platform === "macos") return '"SF Mono", Menlo, monospace';
  if (platform === "windows") return '"Cascadia Mono", Consolas, monospace';
  return '"DejaVu Sans Mono", "Liberation Mono", monospace';
}

export const isMac =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

export const desktopPlatform: DesktopPlatform = isMac
  ? "macos"
  : typeof navigator !== "undefined" && /Win/.test(navigator.platform || navigator.userAgent)
    ? "windows"
    : "linux";
export const modLabel = modifierLabel(desktopPlatform);
export const monospaceFont = monospaceFontFor(desktopPlatform);

export function isModKey(e: KeyboardEvent): boolean {
  return isMac ? e.metaKey : e.ctrlKey;
}
