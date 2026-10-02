import { EmptyState } from "../../components/EmptyState";
import { useActiveDevice } from "../../stores/devices";
import { NoDeviceState } from "../device/NoDeviceState";

/** Views that are implemented in later milestones but must already respect the no-device state. */
export function PlannedView({ title, milestone }: { title: string; milestone: string }) {
  const device = useActiveDevice();
  if (!device) return <NoDeviceState />;
  return (
    <EmptyState title={title}>
      Planned for milestone {milestone}. See docs/MILESTONES.md.
    </EmptyState>
  );
}
