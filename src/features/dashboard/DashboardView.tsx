import { Card } from "../../components/Card";
import { Button } from "../../components/Button";
import { useActiveDevice, useDevices, deviceKey } from "../../stores/devices";
import { useRoute } from "../../stores/route";
import { DeviceInfoCard } from "../device/DeviceInfoCard";
import { HermesStatusCard } from "../hermes/HermesStatusCard";
import { NoDeviceState } from "../device/NoDeviceState";
import { RecentLogs } from "../logs/RecentLogs";

export function DashboardView() {
  const device = useActiveDevice();
  const go = useRoute((state) => state.go);
  const info = useDevices((state) => (device ? state.info[deviceKey(device)]?.data : undefined));
  if (!device) return <NoDeviceState />;
  return (
    <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
      <DeviceInfoCard device={device} />
      <div className="flex flex-col gap-4">
        {info?.termux?.installed === false && (
          <Card
            title="Set up this phone"
            actions={
              <Button variant="primary" onClick={() => go("device")}>
                Open setup wizard
              </Button>
            }
          >
            <p>Termux is not installed on this phone.</p>
          </Card>
        )}
        <HermesStatusCard compact />
        <Card title="Recent logs">
          <RecentLogs />
        </Card>
      </div>
    </div>
  );
}
