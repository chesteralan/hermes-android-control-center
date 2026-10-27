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
import { OverviewView } from "./features/overview/OverviewView";
import { PairDialog } from "./features/device/PairDialog";
import { SettingsView } from "./features/settings/SettingsView";
import { TerminalView } from "./features/terminal/TerminalView";
import { OnboardingView } from "./features/onboarding/OnboardingView";
import { useDevices } from "./stores/devices";
import { useSettings } from "./stores/settings";
import { useRoute, type Route } from "./stores/route";
import { useUi } from "./stores/ui";

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
    case "overview":
      return <OverviewView />;
    case "settings":
      return <SettingsView />;
  }
}

export default function App() {
  const route = useRoute((s) => s.route);
  const { subscribe, detectAdb, refresh } = useDevices();
  const loadSettings = useSettings((s) => s.load);
  const evaluateAutoShow = useUi((s) => s.evaluateAutoShow);
  const onboardingOpen = useUi((s) => s.onboardingOpen);
  const autoStartLogs = useSettings(
    (s) => s.draft?.logs.autoStart ?? s.saved?.logs.autoStart ?? false,
  );

  useEffect(() => {
    const unsubscribe = subscribe();
    void Promise.allSettled([loadSettings(), detectAdb().then(refresh)]).then(() =>
      evaluateAutoShow(Object.keys(useDevices.getState().devices).length),
    );
    return unsubscribe;
  }, [subscribe, detectAdb, refresh, loadSettings, evaluateAutoShow]);

  if (onboardingOpen) {
    return (
      <>
        <OnboardingView />
        <PairDialog />
        <Toasts />
      </>
    );
  }

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
