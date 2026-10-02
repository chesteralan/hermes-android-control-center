import type { Tone } from "../../components/StatusDot";
import type { AndroidDevice, ReconnectStatus } from "../../types";
import { formatSeconds } from "../../lib/format";

export interface StatusView {
  tone: Tone;
  label: string;
  hint?: string;
  pulse?: boolean;
}

export function deviceStatus(d: AndroidDevice, reconnect?: ReconnectStatus): StatusView {
  switch (d.state) {
    case "device":
      return { tone: "success", label: "Connected" };
    case "unauthorized":
      return {
        tone: "warning",
        label: "Unauthorized",
        hint: 'Unlock your phone and accept the "Allow debugging" prompt.',
      };
    case "authorizing":
    case "connecting":
      return { tone: "accent", label: "Connecting", pulse: true };
    case "offline":
      return { tone: "danger", label: "Offline", hint: "Wake the phone and check Wi-Fi." };
    case "reconnecting": {
      const r = reconnect;
      const next = r?.nextDelayMs != null ? `, next in ${formatSeconds(r.nextDelayMs)}` : "";
      return {
        tone: "warning",
        label: r ? `Reconnecting (attempt ${r.attempt}/${r.maxAttempts}${next})` : "Reconnecting",
        pulse: true,
      };
    }
    case "disconnected":
      return { tone: "danger", label: "Disconnected", hint: "Automatic reconnection gave up." };
    case "noPermissions":
      return {
        tone: "danger",
        label: "No permissions",
        hint: "Check USB permissions / udev rules.",
      };
    default:
      return { tone: "muted", label: d.rawState || "Unknown" };
  }
}

export function deviceTitle(d: AndroidDevice): string {
  return d.model?.replace(/_/g, " ") ?? d.deviceId ?? d.serial;
}

export function connectionLabel(d: AndroidDevice): string {
  switch (d.connection) {
    case "usb":
      return "USB";
    case "wirelessIp":
      return "Wi-Fi";
    case "wirelessMdns":
      return "Wi-Fi (mDNS)";
  }
}
