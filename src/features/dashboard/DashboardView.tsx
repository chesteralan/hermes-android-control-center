import { Card } from "../../components/Card";
import { StatusDot } from "../../components/StatusDot";
import { useActiveDevice } from "../../stores/devices";
import { DeviceInfoCard } from "../device/DeviceInfoCard";
import { NoDeviceState } from "../device/NoDeviceState";

export function DashboardView() {
  const device = useActiveDevice();
  if (!device) return <NoDeviceState />;
  return (
    <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
      <DeviceInfoCard device={device} />
      <div className="flex flex-col gap-4">
        <Card title="Hermes Agent">
          <StatusDot tone="muted" label="Unknown" />
          <p className="mt-2 text-muted">
            Hermes status arrives in milestone M5 (requires the Termux bridge).
          </p>
        </Card>
        <Card title="Recent logs">
          <p className="text-muted">Live logs arrive in milestone M3.</p>
        </Card>
      </div>
    </div>
  );
}
