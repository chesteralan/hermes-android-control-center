import { useEffect, useState } from "react";
import { Button } from "../../components/Button";
import { ErrorPanel } from "../../components/ErrorPanel";
import { ipc, toErrorPayload } from "../../lib/ipc";
import { useDeviceProfiles, useProfileList } from "../../stores/deviceProfiles";
import { useSettings } from "../../stores/settings";
import { useToasts } from "../../stores/toast";
import type {
  DeviceProfile,
  DeviceProfileUpdate,
  EffectiveDeviceConfig,
  ErrorPayload,
  HermesConfig,
  TermuxConfig,
} from "../../types";

const input = "w-full rounded-md border border-border bg-bg px-2 py-1.5 font-mono";
const label = "w-40 shrink-0 pt-1.5 text-sm";

// ─── helpers ─────────────────────────────────────────────────────────────────

function Row({
  label: l,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-start gap-4 py-2">
      <span className={label}>
        {l}
        {hint && <span className="block text-[11px] text-muted">{hint}</span>}
      </span>
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}

function ColorSwatch({ color }: { color: string | null }) {
  if (!color) return <span className="text-muted">—</span>;
  return (
    <span
      className="inline-block size-4 rounded-full border border-border"
      style={{ backgroundColor: color }}
      aria-label={color}
    />
  );
}

// ─── override section ─────────────────────────────────────────────────────────

function HermesOverrideFields({
  value,
  global: globalCfg,
  onChange,
  onReset,
}: {
  value: HermesConfig;
  global: HermesConfig;
  onChange: (h: HermesConfig) => void;
  onReset: () => void;
}) {
  const f = (field: keyof HermesConfig) => (e: React.ChangeEvent<HTMLInputElement>) =>
    onChange({ ...value, [field]: e.target.value });

  return (
    <div className="mt-2 space-y-1">
      <Row label="Gateway cmd" hint="Overrides global">
        <input
          aria-label="Hermes gateway command override"
          className={input}
          placeholder={globalCfg.gatewayCommand}
          value={value.gatewayCommand}
          onChange={f("gatewayCommand")}
        />
      </Row>
      <Row label="Log command" hint="Overrides global">
        <input
          aria-label="Hermes log command override"
          className={input}
          placeholder={globalCfg.logCommand || "tail -n 200 -F ~/.hermes/logs/gateway.log"}
          value={value.logCommand}
          onChange={f("logCommand")}
        />
      </Row>
      <Row label="Hermes home" hint="Overrides global">
        <input
          aria-label="Hermes home override"
          className={input}
          placeholder={globalCfg.hermesHome}
          value={value.hermesHome}
          onChange={f("hermesHome")}
        />
      </Row>
      <div className="flex justify-end pt-1">
        <Button variant="ghost" onClick={onReset}>
          Reset Hermes to global
        </Button>
      </div>
    </div>
  );
}

function TermuxOverrideFields({
  value,
  global: globalCfg,
  onChange,
  onReset,
}: {
  value: TermuxConfig;
  global: TermuxConfig;
  onChange: (t: TermuxConfig) => void;
  onReset: () => void;
}) {
  return (
    <div className="mt-2 space-y-1">
      <Row label="SSH user" hint="Overrides global">
        <input
          aria-label="Termux SSH user override"
          className={input}
          placeholder={globalCfg.sshUser}
          value={value.sshUser}
          onChange={(e) => onChange({ ...value, sshUser: e.target.value })}
        />
      </Row>
      <Row label="SSH port" hint="Overrides global">
        <input
          type="number"
          aria-label="Termux SSH port override"
          className={`${input} w-28`}
          placeholder={String(globalCfg.sshPort)}
          value={value.sshPort}
          min={1}
          max={65535}
          onChange={(e) => onChange({ ...value, sshPort: Number(e.target.value) })}
        />
      </Row>
      <div className="flex justify-end pt-1">
        <Button variant="ghost" onClick={onReset}>
          Reset Termux to global
        </Button>
      </div>
    </div>
  );
}

// ─── edit form ────────────────────────────────────────────────────────────────

interface EditFormProps {
  profile: DeviceProfile;
  effective: EffectiveDeviceConfig | undefined;
  globalHermes: HermesConfig;
  globalTermux: TermuxConfig;
  onSaved: () => void;
  onCancel: () => void;
}

function EditForm({
  profile,
  effective,
  globalHermes,
  globalTermux,
  onSaved,
  onCancel,
}: EditFormProps) {
  const profileStore = useDeviceProfiles();
  const toast = useToasts((t) => t.push);

  const [alias, setAlias] = useState(profile.alias ?? "");
  const [color, setColor] = useState(profile.color ?? "");
  const [autoConnect, setAutoConnect] = useState(profile.autoConnect);
  const [lastAddress, setLastAddress] = useState(profile.lastAddress ?? "");

  const [hermesOpen, setHermesOpen] = useState(profile.hermes !== null);
  const [hermesOverride, setHermesOverride] = useState<HermesConfig | null>(profile.hermes ?? null);

  const [termuxOpen, setTermuxOpen] = useState(profile.termux !== null);
  const [termuxOverride, setTermuxOverride] = useState<TermuxConfig | null>(profile.termux ?? null);

  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<ErrorPayload | null>(null);

  function toggleHermes(checked: boolean) {
    setHermesOpen(checked);
    if (checked && hermesOverride === null) {
      setHermesOverride(effective?.hermes ?? globalHermes);
    }
    if (!checked) {
      setHermesOverride(null);
    }
  }

  function toggleTermux(checked: boolean) {
    setTermuxOpen(checked);
    if (checked && termuxOverride === null) {
      setTermuxOverride(effective?.termux ?? globalTermux);
    }
    if (!checked) {
      setTermuxOverride(null);
    }
  }

  async function handleSave() {
    setSaving(true);
    setError(null);
    try {
      const update: DeviceProfileUpdate = {
        alias: alias.trim() ? alias.trim() : null,
        color: color.trim() ? color.trim() : null,
        autoConnect,
        lastAddress: lastAddress.trim() ? lastAddress.trim() : null,
        hermes: hermesOverride,
        termux: termuxOverride,
        logCommand: undefined,
      };
      await profileStore.set(profile.deviceId, update);
      toast("Device profile saved", "success");
      onSaved();
    } catch (e) {
      setError(toErrorPayload(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="space-y-1 py-2">
      <Row label="Alias">
        <input
          aria-label="Device alias"
          className={input}
          placeholder="e.g. My Pixel 7"
          value={alias}
          onChange={(e) => setAlias(e.target.value)}
        />
      </Row>
      <Row label="Color" hint="CSS color or hex, e.g. #3b82f6">
        <div className="flex items-center gap-2">
          <input
            aria-label="Device color"
            className={`${input} w-36`}
            placeholder="#3b82f6"
            value={color}
            onChange={(e) => setColor(e.target.value)}
          />
          {color && (
            <span
              className="inline-block size-5 shrink-0 rounded-full border border-border"
              style={{ backgroundColor: color }}
              aria-hidden
            />
          )}
        </div>
      </Row>
      <Row label="Last address" hint="e.g. 192.0.2.10:5555">
        <input
          aria-label="Device last address"
          className={input}
          placeholder="192.0.2.10:5555"
          value={lastAddress}
          onChange={(e) => setLastAddress(e.target.value)}
        />
      </Row>
      <Row label="Auto-connect">
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            aria-label="Auto-connect"
            checked={autoConnect}
            onChange={(e) => setAutoConnect(e.target.checked)}
          />
          Reconnect automatically when device is seen
        </label>
      </Row>

      {/* Hermes override */}
      <div className="mt-3 rounded-md border border-border">
        <button
          type="button"
          className="flex w-full items-center justify-between px-3 py-2 text-left"
          onClick={() => toggleHermes(!hermesOpen)}
          aria-expanded={hermesOpen}
        >
          <span className="font-medium">Hermes overrides</span>
          <span className="text-muted text-xs">{hermesOpen ? "▲ collapse" : "▼ expand"}</span>
        </button>
        {hermesOpen ? (
          <div className="border-t border-border px-3 pb-3">
            <p className="pt-2 text-[12px] text-muted">
              Override the global Hermes settings for this device only. Leave blank to inherit
              global values.
            </p>
            {hermesOverride && (
              <HermesOverrideFields
                value={hermesOverride}
                global={globalHermes}
                onChange={setHermesOverride}
                onReset={() => {
                  setHermesOverride(null);
                  setHermesOpen(false);
                }}
              />
            )}
          </div>
        ) : (
          <div className="border-t border-border px-3 py-2 text-muted text-xs">
            Using global Hermes config.{" "}
            <button type="button" className="underline" onClick={() => toggleHermes(true)}>
              Add override
            </button>
          </div>
        )}
      </div>

      {/* Termux override */}
      <div className="mt-2 rounded-md border border-border">
        <button
          type="button"
          className="flex w-full items-center justify-between px-3 py-2 text-left"
          onClick={() => toggleTermux(!termuxOpen)}
          aria-expanded={termuxOpen}
        >
          <span className="font-medium">Termux overrides</span>
          <span className="text-muted text-xs">{termuxOpen ? "▲ collapse" : "▼ expand"}</span>
        </button>
        {termuxOpen ? (
          <div className="border-t border-border px-3 pb-3">
            <p className="pt-2 text-[12px] text-muted">
              Override SSH user/port for this device only.
            </p>
            {termuxOverride && (
              <TermuxOverrideFields
                value={termuxOverride}
                global={globalTermux}
                onChange={setTermuxOverride}
                onReset={() => {
                  setTermuxOverride(null);
                  setTermuxOpen(false);
                }}
              />
            )}
          </div>
        ) : (
          <div className="border-t border-border px-3 py-2 text-muted text-xs">
            Using global Termux config.{" "}
            <button type="button" className="underline" onClick={() => toggleTermux(true)}>
              Add override
            </button>
          </div>
        )}
      </div>

      {error && <ErrorPanel error={error} />}

      <div className="flex justify-end gap-2 pt-3">
        <Button variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button variant="primary" loading={saving} onClick={() => void handleSave()}>
          Save
        </Button>
      </div>
    </div>
  );
}

// ─── main section ─────────────────────────────────────────────────────────────

export function DeviceProfilesSection() {
  const profileStore = useDeviceProfiles();
  const profiles = useProfileList();
  const globalSettings = useSettings((s) => s.saved);

  const [editingId, setEditingId] = useState<string | null>(null);
  const [effectiveMap, setEffectiveMap] = useState<Record<string, EffectiveDeviceConfig>>({});
  const [loadingEffective, setLoadingEffective] = useState<Record<string, boolean>>({});

  const { load } = profileStore;

  useEffect(() => {
    void load();
  }, [load]);

  async function openEdit(deviceId: string) {
    setEditingId(deviceId);
    if (!effectiveMap[deviceId] && !loadingEffective[deviceId]) {
      setLoadingEffective((m) => ({ ...m, [deviceId]: true }));
      try {
        const eff = await ipc.getEffectiveDeviceConfig(deviceId);
        setEffectiveMap((m) => ({ ...m, [deviceId]: eff }));
      } catch {
        // non-critical; fall back to globals
      } finally {
        setLoadingEffective((m) => ({ ...m, [deviceId]: false }));
      }
    }
  }

  function handleSaved() {
    setEditingId(null);
    void load();
  }

  if (profileStore.loading && profiles.length === 0) {
    return <p className="py-2 text-muted text-sm">Loading device profiles…</p>;
  }

  if (profiles.length === 0) {
    return (
      <p className="py-2 text-muted text-sm">
        No device profiles yet. Profiles are created automatically when a device connects.
      </p>
    );
  }

  const globalHermes = globalSettings?.hermes;
  const globalTermux = globalSettings?.termux;

  return (
    <div className="space-y-2">
      {profileStore.error && <ErrorPanel error={profileStore.error} onRetry={() => void load()} />}
      {profiles.map((p) => (
        <div key={p.deviceId} className="rounded-md border border-border">
          {/* Profile list row */}
          <div className="flex items-center justify-between px-3 py-2">
            <div className="flex items-center gap-3 min-w-0">
              <ColorSwatch color={p.color} />
              <div className="min-w-0">
                <p className="truncate font-medium">
                  {p.alias ?? <span className="text-muted italic">{p.deviceId}</span>}
                </p>
                <p className="text-[11px] text-muted font-mono truncate">
                  {p.deviceId}
                  {p.lastAddress ? ` · ${p.lastAddress}` : ""}
                </p>
              </div>
            </div>
            <div className="flex items-center gap-3 shrink-0 ml-3">
              {p.autoConnect ? (
                <span className="text-[11px] text-success">auto-connect</span>
              ) : (
                <span className="text-[11px] text-muted">manual</span>
              )}
              {p.hermes && (
                <span
                  className="rounded bg-accent/10 px-1.5 py-0.5 text-[10px] text-accent"
                  title="Has Hermes override"
                >
                  Hermes↑
                </span>
              )}
              {p.termux && (
                <span
                  className="rounded bg-accent/10 px-1.5 py-0.5 text-[10px] text-accent"
                  title="Has Termux override"
                >
                  Termux↑
                </span>
              )}
              <Button
                variant="ghost"
                aria-label={`Edit profile for ${p.alias ?? p.deviceId}`}
                onClick={() =>
                  void (editingId === p.deviceId ? setEditingId(null) : openEdit(p.deviceId))
                }
              >
                {editingId === p.deviceId ? "Close" : "Edit"}
              </Button>
            </div>
          </div>

          {/* Edit form, expanded inline */}
          {editingId === p.deviceId && globalHermes && globalTermux && (
            <div className="border-t border-border px-3">
              {loadingEffective[p.deviceId] ? (
                <p className="py-3 text-muted text-sm">Loading effective config…</p>
              ) : (
                <EditForm
                  profile={p}
                  effective={effectiveMap[p.deviceId]}
                  globalHermes={globalHermes}
                  globalTermux={globalTermux}
                  onSaved={handleSaved}
                  onCancel={() => setEditingId(null)}
                />
              )}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
