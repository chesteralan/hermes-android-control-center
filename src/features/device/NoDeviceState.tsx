import { Button } from "../../components/Button";
import { EmptyState } from "../../components/EmptyState";
import { useRoute } from "../../stores/route";
import { useUi } from "../../stores/ui";

export function NoDeviceState() {
  const openPair = useUi((s) => s.openPair);
  const go = useRoute((s) => s.go);
  return (
    <EmptyState title="No phone connected">
      <p className="mb-4">Pair a phone once, then connect over Wi-Fi with Wireless debugging.</p>
      <div className="flex flex-wrap justify-center gap-2">
        <Button variant="primary" onClick={() => openPair("qr")}>
          Pair a phone to set it up
        </Button>
        <Button onClick={() => openPair("code")}>Pair with code</Button>
        <Button onClick={() => go("device")}>Connect by IP</Button>
        <Button onClick={() => go("device")}>Discover</Button>
      </div>
      <p className="mt-4 text-[12px]">
        See docs/guides/ANDROID_SETUP.md for enabling Wireless debugging.
      </p>
    </EmptyState>
  );
}
