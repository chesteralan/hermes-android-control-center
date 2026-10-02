import { Button } from "../../components/Button";
import { ErrorPanel } from "../../components/ErrorPanel";
import { useDevices } from "../../stores/devices";
import { useRoute } from "../../stores/route";

/** Shown app-wide when adb can't be found; takes priority over the no-device state. */
export function AdbBanner() {
  const adb = useDevices((s) => s.adb);
  const detect = useDevices((s) => s.detectAdb);
  const go = useRoute((s) => s.go);
  if (!adb.checked || adb.info || !adb.error) return null;
  return (
    <div className="flex flex-col gap-2 border-b border-border bg-danger/5 p-3">
      <ErrorPanel error={adb.error} />
      <div className="flex gap-2">
        <Button onClick={() => void detect()}>Detect again</Button>
        <Button variant="ghost" onClick={() => go("settings")}>
          Set ADB path
        </Button>
      </div>
    </div>
  );
}
