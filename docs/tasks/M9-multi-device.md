# M9 — Multi-Device (v0.6.0)

**Goal:** Manage several phones at once, one tab per phone, with per-phone profiles. **Depends on:** M7 (M6 is parallel-friendly). **Design:** [ADR-012](../DECISIONS.md#adr-012-multiple-phones-one-tab-per-phone), [ARCHITECTURE §11](../ARCHITECTURE.md#11-multiple-devices). **Unblocks:** M11-T2/T14 per-phone HermesConfig persistence and the M11 wizard profile handoff.

## Scope

- Stable `device_id` per phone (already learned from M2-T5a); every map, buffer, session, history entry, host key, secret reference, and profile is keyed by `device_id`, never by wireless serial.
- Per-phone `DeviceProfile`: alias, color, last address, auto-connect, Hermes/Termux override configs, log command. `effective_config(device_id) = profile override ?? global default`.
- Device tab bar; each tab keeps its own view, terminal sessions, and log buffer.
- Overview view with status cards for all phones.
- Independent reconnect per phone; one failing or hung phone never blocks the others.
- Every destructive action names the target phone (alias + model + address) in its confirmation.

## Non-goals

- Simultaneous broadcast of arbitrary terminal commands. Broadcast is limited to predefined actions/snippets with explicit multi-select and confirmation.
- Per-phone installation of separate desktop app instances or separate ADB servers. One `adb` server and one `track-devices` stream serve all phones (shared-resource warning in UI before server restart).
- Phone grouping, rooms, or fleet-wide orchestration beyond the Overview grid.
- Changing wireless pairing/identity semantics: `device_id` resolution stays `ro.serialno` → `ro.boot.serialno` → `android_id` (ADR-012).

## Groundwork already in place (earlier milestones)

- `AndroidDevice.device_id` learned on connect; `device_id_from_serial` fallback for mDNS serials (M2-T5a).
- Frontend `deviceStore` keyed by `deviceKey = deviceId ?? serial` with `activeKey` (M2).
- SSH host keys TOFU-pinned per `device_id` (M4); API tokens and encrypted secrets per `device_id` (M8); provisioning progress and Termux-install receipts per `device_id` (M11).
- Device + stream registries recover poisoned locks; reconnect registry is per device (M10-T1 work).

## Tasks

### [~] M9-T1 Backend isolation audit and completion
- Confirm/complete `DeviceSession` boundaries per ARCHITECTURE §11: each phone owns its transports, `adb forward` ports (`tcp:0`), Hermes manager, reconnect supervisor, and streams. No lock is held across I/O that another phone's session needs.
- Move `ActionLocks` (M5-T3) from serial-keyed to stable `device_id`-keyed so a wireless port change cannot split or double-run actions.
- Move frontend per-device stores (terminal history, log ring buffer, reconnect status, Hermes polling) to consistent `device_id` keys where serial keys remain (e.g. `reconnect` map).
- Verify `cancel_device`/stream teardown targets only the named phone; restarting the shared ADB server emits the shared-resource warning and is the only cross-phone interrupt.
- **Tests:** serial rebind keeps action locks and streams attached to the same `device_id`; one device's hung SSH connect does not delay another device's status; registry poison recovery remains covered.

### [~] M9-T2 `DeviceProfile` schema, persistence, and effective config
- Implement `devices: HashMap<device_id, DeviceProfile>` in `AppConfig` with schema `version` migration: existing remembered addresses and auto-connect flags migrate into profiles on first load.
- `effective_config(device_id)` merge helper: per-field fallback (`None` override inherits the global Hermes/Termux/log defaults). Documented in ARCHITECTURE §9.
- Profile CRUD commands: `get_device_profile`, `set_device_profile` (partial update), `list_device_profiles`. Never log override values that may contain secrets.
- **Unblocks:** M11-T2/T14 save the wizard's recipe-derived `HermesConfig` into the phone's profile instead of the global config.
- **Tests:** migration of legacy remembered-address settings; merge precedence per field; partial update preserves unrelated fields; unknown `device_id` errors clearly.

### [~] M9-T3 Profile editor UI (Settings)
- Devices section listing known profiles (alias, color swatch, last address, auto-connect toggle).
- Edit form: alias, color, auto-connect, last address; collapsible Hermes and Termux override sections prefilled from globals with a "Reset to global" control per field.
- Explicit save; dirty-state guard; validation errors inline via the shared error pattern.
- **Tests:** form renders current effective values vs overrides distinctly; save emits partial updates; reset clears override.

### [~] M9-T4 Device tab bar
- Header tab per connected/known phone: color swatch, alias or model fallback, ● connection state; overflow menu (Connect/Disconnect, Retry, Open profile).
- Selecting a tab sets `activeKey` and switches the per-device route view (Dashboard/Device/Hermes/Termux/Terminal/Logs) without losing the other tab's view state.
- Single phone keeps the current layout without a forced tab strip. Tabs persist order and last-selected tab across restart (by `device_id`).
- **Tests:** tab rendering for mixed states (connected, reconnecting, offline, unknown); selection persistence; port-change rebind keeps the same tab.

### [~] M9-T5 Per-tab terminal and log isolation
- Terminal sessions and scrollback are scoped per `device_id` (and transport); switching tabs never merges output. Closing a tab offers to end that phone's streams/sessions.
- Log viewer buffers, search, and pause state are per phone; the ring-buffer limit in LimitsConfig applies per device.
- **Tests:** interleaved output from two fake devices stays separated; buffer bounds are per device; cancel-on-close affects only the closed tab's streams.

### [~] M9-T6 Overview view
- Global view with a status card per phone: alias/color, model, connection state, battery/storage snapshot when known, Hermes gateway state when known, latest reconnect attempt.
- Cards link to the phone's tab. Refresh follows the existing per-device polling cadence; a slow phone shows its own spinner without blocking others.
- **Tests:** cards render mixed availability (no info yet, error, connected); one device's pending promise does not gate another's render.

### [~] M9-T7 Independent reconnect verification
- Reconnection uses each phone's remembered profile address with the existing backoff schedule; giving up on one phone must not stop retries on another.
- Reconnect events (`device://reconnect`) carry `device_id`; UI attributes attempts to the right tab/card.
- **Tests:** two concurrent failing reconnect loops advance independently (fake clock); give-up on A leaves B retrying; manual Retry re-arms only the selected phone.

### [~] M9-T8 Destructive actions and limited broadcast
- Every confirmation dialog (Stop/Restart Hermes, disconnect, provisioning actions, snippet execution) shows the target phone's alias, model, and address.
- Optional broadcast: predefined actions/snippets only, explicit multi-select of phones, one confirmation listing every target, per-phone result report. No free-form command broadcast.
- **Tests:** confirm payload includes the target identity; broadcast confirmation lists all selected phones; one phone's failure does not cancel or mislabel the others' results.

### [!] M9-T9 Multi-device QA
- Run the TESTING.md multi-device rows: 3 phones streaming logs; drop/reconnect one phone; actions always hit the targeted phone; resource limits hold.
- Cross-check that M11 wizard saves profiles per phone (hands off to M11-T2/T14 once implemented).
- **Live check:** requires ≥ 2 connected Android phones; record results in the exit check.

## Exit check

- [ ] Two or more phones appear as tabs; each keeps its own view, terminal sessions, and log buffer across switching and app restart.
- [ ] Per-phone profiles (alias, color, Hermes/Termux overrides, auto-connect) persist and take effect via `effective_config`; legacy settings migrated.
- [ ] Wireless port change on one phone rebinds its tab, sessions, host key, and history to the same `device_id`.
- [ ] One phone offline/hung never blocks the other phones' status, streams, or reconnect attempts (verified with fakes and live).
- [ ] Overview shows a live card per phone.
- [ ] Every destructive confirmation names the target phone; broadcast is limited to predefined actions with multi-select.
- [ ] M11-T2/T14 can persist per-phone HermesConfig (dependency verified).
