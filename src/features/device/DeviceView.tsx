import { Card } from "../../components/Card";
import { ErrorPanel } from "../../components/ErrorPanel";
import { useActiveDevice, useDevices } from "../../stores/devices";
import { ConnectPanel } from "./ConnectPanel";
import { DeviceInfoCard } from "./DeviceInfoCard";
import { DeviceList } from "./DeviceList";
import { TermuxSetupCard } from "../termux/TermuxSetupCard";

export function DeviceView() {
  const active = useActiveDevice();
  const count = useDevices((s) => Object.keys(s.devices).length);
  const listError = useDevices((s) => s.listError);
  const refresh = useDevices((s) => s.refresh);
  return (
    <div className="flex flex-col gap-4">
      <Card title="Connect">
        <ConnectPanel />
      </Card>
      {listError && <ErrorPanel error={listError} onRetry={refresh} />}
      {count > 0 && (
        <Card title={`Devices (${count})`}>
          <DeviceList />
        </Card>
      )}
      {active && <DeviceInfoCard device={active} />}
      {active && <TermuxSetupCard device={active} />}
    </div>
  );
}
