import { useEffect } from "react";
import { Layout } from "./app/Layout";
import { Toasts } from "./components/Toasts";
import { AdbBanner } from "./features/adb/AdbBanner";
import { DashboardView } from "./features/dashboard/DashboardView";
import { DeviceView } from "./features/device/DeviceView";
import { LogsView } from "./features/logs/LogsView";
import { PairDialog } from "./features/device/PairDialog";
import { PlannedView } from "./features/placeholder/PlannedView";
import { SettingsView } from "./features/settings/SettingsView";
import { TerminalView } from "./features/terminal/TerminalView";
import { useDevices } from "./stores/devices";
import { useRoute, type Route } from "./stores/route";

function View({ route }: { route: Route }) {
  switch (route) {
    case "dashboard":
      return <DashboardView />;
    case "device":
      return <DeviceView />;
    case "hermes":
      return <PlannedView title="Hermes" milestone="M5" />;
    case "terminal":
      return <TerminalView />;
    case "logs":
      return <LogsView />;
    case "settings":
      return <SettingsView />;
  }
}

export default function App() {
  const route = useRoute((s) => s.route);
  const { subscribe, detectAdb, refresh } = useDevices();

  useEffect(() => {
    const unsubscribe = subscribe();
    void detectAdb().then(refresh);
    return unsubscribe;
  }, [subscribe, detectAdb, refresh]);

  return (
    <>
      <Layout>
        <AdbBanner />
        <div className="p-6">
          <View route={route} />
        </div>
      </Layout>
      <PairDialog />
      <Toasts />
    </>
  );
}
