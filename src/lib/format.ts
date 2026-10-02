export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "Unknown";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 10 || i === 0 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

export function formatSeconds(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  return s >= 60 ? `${Math.floor(s / 60)}m ${s % 60}s` : `${s}s`;
}

export const UNKNOWN = "Unknown";

export function orUnknown(v: string | number | null | undefined): string {
  return v === null || v === undefined || v === "" ? UNKNOWN : String(v);
}

const IPV4 = /^(25[0-5]|2[0-4]\d|1?\d?\d)(\.(25[0-5]|2[0-4]\d|1?\d?\d)){3}$/;
const HOST = /^[a-zA-Z0-9][a-zA-Z0-9.-]*$/;

/** Mirrors the Rust validation so the user gets instant feedback. */
export function validateHost(host: string): string | null {
  const h = host.trim();
  if (!h) return "Enter the phone's IP address.";
  if (/^[\d.]+$/.test(h)) return IPV4.test(h) ? null : "Invalid IPv4 address.";
  return HOST.test(h) ? null : "Invalid host name.";
}

export function validatePort(port: string): string | null {
  const p = port.trim();
  if (!/^\d+$/.test(p)) return "Port must be a number.";
  const n = Number(p);
  return n >= 1 && n <= 65535 ? null : "Port must be between 1 and 65535.";
}
