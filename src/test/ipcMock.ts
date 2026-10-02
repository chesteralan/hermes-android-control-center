import { invoke } from "@tauri-apps/api/core";
import { vi } from "vitest";
import { useDevices } from "../stores/devices";
import { useRoute } from "../stores/route";
import { useSettings } from "../stores/settings";
import { useUi } from "../stores/ui";

type Handler = (args: Record<string, unknown> | undefined) => unknown;

/** Route `invoke(cmd, args)` to per-command handlers; unknown commands reject loudly. */
export function mockIpc(handlers: Record<string, Handler>) {
  const fn = vi.mocked(invoke);
  fn.mockImplementation(async (cmd: string, args?: unknown) => {
    const h = handlers[cmd];
    if (!h) throw { kind: "io", message: `unmocked command ${cmd}`, details: null };
    return h(args as Record<string, unknown> | undefined);
  });
  return fn;
}

export function resetStores() {
  useDevices.setState({
    devices: {},
    activeKey: null,
    reconnect: {},
    info: {},
    adb: { info: null, error: null, checked: false },
    listError: null,
  });
  useRoute.setState({ route: "dashboard" });
  useSettings.setState({ saved: null, draft: null, error: null, saving: false });
  useUi.setState({ pairTab: null });
}
