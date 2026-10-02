import { Button } from "../../components/Button";
import { deviceKey, useActiveDevice } from "../../stores/devices";
import { useLogs } from "../../stores/logs";
import { useRoute } from "../../stores/route";
import { LogRow } from "./LogsView";

const RECENT = 20;

export function RecentLogs() {
  const device = useActiveDevice();
  const key = device ? deviceKey(device) : "";
  const lines = useLogs((s) => s.byDevice[key]?.lines);
  const go = useRoute((s) => s.go);
  const recent = (lines ?? []).slice(-RECENT);
  if (recent.length === 0) {
    return (
      <div className="flex items-center justify-between text-muted">
        No log stream running.
        <Button onClick={() => go("logs")}>Open logs</Button>
      </div>
    );
  }
  return (
    <div className="-mx-4 -my-2 overflow-hidden">
      {recent.map((l) => (
        <LogRow key={l.seq} line={l} />
      ))}
    </div>
  );
}
