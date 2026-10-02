import { Card } from "../../components/Card";
import { useActiveDevice } from "../../stores/devices";
import { DeviceInfoCard } from "../device/DeviceInfoCard";
import { HermesStatusCard } from "../hermes/HermesStatusCard";
import { NoDeviceState } from "../device/NoDeviceState";
import { RecentLogs } from "../logs/RecentLogs";

export function DashboardView() {
  const device = useActiveDevice();
  if (!device) return <NoDeviceState />;
  return (
    <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
      <DeviceInfoCard device={device} />
      <div className="flex flex-col gap-4">
        <HermesStatusCard compact />
        <Card title="Recent logs">
          <RecentLogs />
        </Card>
      </div>
    </div>
  );
}
