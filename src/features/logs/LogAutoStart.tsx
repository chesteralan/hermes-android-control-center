import { useEffect } from "react";
import { ipc } from "../../lib/ipc";
import { deviceKey, useDevices } from "../../stores/devices";
import { useLogs } from "../../stores/logs";

interface LogAutoStartProps {
  enabled: boolean;
}

export function LogAutoStart({ enabled }: LogAutoStartProps) {
  const connectedSignature = useDevices((state) =>
    JSON.stringify(
      Object.values(state.devices)
        .filter((device) => device.state === "device")
        .map((device) => `${deviceKey(device)}:${device.serial}`)
        .sort(),
    ),
  );

  useEffect(() => {
    if (!enabled) return;
    let active = true;
    const candidates = Object.values(useDevices.getState().devices).filter(
      (device) => device.state === "device",
    );

    for (const device of candidates) {
      const key = deviceKey(device);
      const current = useLogs.getState().byDevice[key];
      if (current?.streamId || current?.starting) continue;

      void ipc
        .checkTermux(device.serial)
        .then((check) => {
          if (!active || !check.ready) return;
          const connected = useDevices.getState().devices[key];
          if (!connected || connected.serial !== device.serial || connected.state !== "device") {
            return;
          }
          const latest = useLogs.getState().byDevice[key];
          if (latest?.streamId || latest?.starting) return;
          void useLogs.getState().start(key, device.serial, "hermesGateway");
        })
        .catch(() => {});
    }

    return () => {
      active = false;
    };
  }, [connectedSignature, enabled]);

  return null;
}