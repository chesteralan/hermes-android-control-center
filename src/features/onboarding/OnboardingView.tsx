import { useEffect, useState } from "react";
import { Button } from "../../components/Button";
import { Card } from "../../components/Card";
import { ErrorPanel } from "../../components/ErrorPanel";
import { StatusDot } from "../../components/StatusDot";
import { ipc, toErrorPayload } from "../../lib/ipc";
import { useActiveDevice, useDevices } from "../../stores/devices";
import { useRoute } from "../../stores/route";
import { useUi } from "../../stores/ui";
import type { AdbInfo } from "../../types";
import { deviceTitle } from "../device/deviceStatus";

type Step = "adb" | "connect" | "choice" | "checks";

type CheckRow = { state: "pass" | "fail"; message: string | null } | { state: "running" };

type ChecksState = { termux: CheckRow; hermes: CheckRow };

const STEP_TITLES: Record<Step, string> = {
  adb: "1. Find ADB",
  connect: "2. Connect a phone",
  choice: "3. Choose setup",
  checks: "4. Verify setup",
};

function deriveStep(adbInfo: AdbInfo | null, hasDevice: boolean, override: Step | null): Step {
  if (override) return override;
  if (!adbInfo) return "adb";
  return hasDevice ? "choice" : "connect";
}

function isGatewayUsable(gateway: string): boolean {
  return gateway === "running" || gateway === "degraded";
}

function checkRow(ready: boolean): CheckRow {
  return { state: ready ? "pass" : "fail", message: null };
}

function failedRow(e: unknown): CheckRow {
  return { state: "fail", message: toErrorPayload(e).message };
}

/** First-run setup guide rendered instead of the app shell until dismissed. */
export function OnboardingView() {
  const adb = useDevices((s) => s.adb);
  const detectAdb = useDevices((s) => s.detectAdb);
  const active = useActiveDevice();
  const go = useRoute((s) => s.go);
  const openPair = useUi((s) => s.openPair);
  const closeOnboarding = useUi((s) => s.closeOnboarding);
  const dismissOnboarding = useUi((s) => s.dismissOnboarding);

  const activeSerial = active?.serial ?? null;
  const [stepOverride, setStepOverride] = useState<Step | null>(null);
  const [checks, setChecks] = useState<ChecksState | null>(null);
  const step = deriveStep(adb.info, active !== null, stepOverride);

  useEffect(() => {
    if (step !== "checks" || !activeSerial) return;
    let cancelled = false;
    void Promise.allSettled([
      ipc.checkTermux(activeSerial),
      ipc.getHermesStatus(activeSerial),
    ]).then(([termuxResult, hermesResult]) => {
      if (cancelled) return;
      setChecks({
        termux:
          termuxResult.status === "fulfilled"
            ? checkRow(termuxResult.value.ready)
            : failedRow(termuxResult.reason),
        hermes:
          hermesResult.status === "fulfilled"
            ? checkRow(isGatewayUsable(hermesResult.value.gateway))
            : failedRow(hermesResult.reason),
      });
    });
    return () => {
      cancelled = true;
    };
  }, [step, activeSerial]);

  const bothDone =
    checks !== null && checks.termux.state !== "running" && checks.hermes.state !== "running";
  const bothPass = bothDone && checks.termux.state === "pass" && checks.hermes.state === "pass";

  return (
    <div className="flex min-h-screen items-center justify-center bg-bg p-6">
      <div className="w-full max-w-xl">
        <h1 className="text-[20px] font-semibold">Set up your first phone</h1>
        <p className="mt-1 text-[13px] text-muted">
          Pair a phone, then set it up automatically or verify it is ready to use.
        </p>
        <Card className="mt-4" title={STEP_TITLES[step]}>
          {step === "adb" && (
            <>
              {adb.error && <ErrorPanel error={adb.error} />}
              <div className="mt-3 flex gap-2">
                <Button onClick={() => void detectAdb()}>Detect again</Button>
                <Button
                  variant="primary"
                  onClick={() => {
                    closeOnboarding();
                    go("settings");
                  }}
                >
                  Set ADB path
                </Button>
              </div>
            </>
          )}

          {step === "connect" && (
            <>
              <p className="text-[13px] text-muted">
                Pair a phone over Wireless debugging to connect it to the app.
              </p>
              <div className="mt-3 flex gap-2">
                <Button variant="primary" onClick={() => openPair("qr")}>
                  Pair a phone to set it up
                </Button>
                <Button onClick={() => openPair("code")}>Pair with code</Button>
              </div>
              <p className="mt-4 text-[12px] text-muted">
                See docs/guides/ANDROID_SETUP.md for enabling Wireless debugging.
              </p>
            </>
          )}

          {step === "choice" && (
            <>
              <p className="text-[13px] text-muted">
                {active ? deviceTitle(active) : "Your phone"} is connected. How should it be set up?
              </p>
              <div className="mt-3 flex gap-2">
                <Button
                  variant="primary"
                  onClick={() => {
                    dismissOnboarding();
                    go("device");
                  }}
                >
                  Set up this phone automatically
                </Button>
                <Button
                  onClick={() => {
                    setChecks({ termux: { state: "running" }, hermes: { state: "running" } });
                    setStepOverride("checks");
                  }}
                >
                  This phone is already set up
                </Button>
              </div>
            </>
          )}

          {step === "checks" &&
            (checks === null || !bothDone ? (
              <Button loading>Checking setup</Button>
            ) : (
              <>
                <ul className="space-y-2 text-[13px]">
                  <li>
                    <StatusDot
                      tone={checks.termux.state === "pass" ? "success" : "danger"}
                      label={
                        checks.termux.state === "pass" ? "Termux is set up" : "Termux is not ready"
                      }
                    />
                    {checks.termux.state === "fail" && checks.termux.message && (
                      <span className="ml-4 text-danger">{checks.termux.message}</span>
                    )}
                  </li>
                  <li>
                    <StatusDot
                      tone={checks.hermes.state === "pass" ? "success" : "danger"}
                      label={
                        checks.hermes.state === "pass"
                          ? "Hermes is running"
                          : "Hermes is not running"
                      }
                    />
                    {checks.hermes.state === "fail" && checks.hermes.message && (
                      <span className="ml-4 text-danger">{checks.hermes.message}</span>
                    )}
                  </li>
                </ul>
                <div className="mt-3 flex gap-2">
                  <Button variant="primary" onClick={dismissOnboarding}>
                    {bothPass ? "Finish" : "Finish anyway"}
                  </Button>
                  {!bothPass && (
                    <Button
                      onClick={() => {
                        dismissOnboarding();
                        go("device");
                      }}
                    >
                      Open setup wizard
                    </Button>
                  )}
                </div>
              </>
            ))}
        </Card>
        <div className="mt-4 flex justify-end">
          <Button variant="ghost" onClick={dismissOnboarding}>
            Skip for now
          </Button>
        </div>
      </div>
    </div>
  );
}
