import { useEffect } from "react";
import { Layout } from "./app/Layout";
import { Toasts } from "./components/Toasts";
import { AdbBanner } from "./features/adb/AdbBanner";
import { DashboardView } from "./features/dashboard/DashboardView";
import { DeviceView } from "./features/device/DeviceView";
import { LogsView } from "./features/logs/LogsView";
import { LogAutoStart } from "./features/logs/LogAutoStart";
import { HermesStatusCard } from "./features/hermes/HermesStatusCard";
import { ChatView } from "./features/chat/ChatView";
import { PairDialog } from "./features/device/PairDialog";
import { SettingsView } from "./features/settings/SettingsView";
import { TerminalView } from "./features/terminal/TerminalView";
import { useDevices } from "./stores/devices";
import { useSettings } from "./stores/settings";
import { useRoute, type Route } from "./stores/route";

function View({ route }: { route: Route }) {
  switch (route) {
    case "dashboard":
      return <DashboardView />;
    case "device":
      return <DeviceView />;
    case "hermes":
      return <HermesStatusCard />;
    case "chat":
      return <ChatView />;
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
  const loadSettings = useSettings((s) => s.load);
  const autoStartLogs = useSettings(
    (s) => s.draft?.logs.autoStart ?? s.saved?.logs.autoStart ?? false,
  );

  useEffect(() => {
    const unsubscribe = subscribe();
    void loadSettings();
    void detectAdb().then(refresh);
    return unsubscribe;
  }, [subscribe, detectAdb, refresh, loadSettings]);

  return (
    <>
      <LogAutoStart enabled={autoStartLogs} />
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
