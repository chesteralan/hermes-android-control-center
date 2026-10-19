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
- **Regression (live QA 2026-10-19):** the migration keyed profiles by **address string**, not `device_id` — config v3 contained nine profiles such as `"192.168.0.107:43237"` with `deviceId: "192.168.0.107:43237"`, and no profile under either phone's `ro.serialno`. Settings → DEVICES listed the ghost addresses with no alias/color for the real phones; `profiles[deviceKey(device)]` therefore missed for every connected phone (tab labels fell back to model, `effective_config` missed, supervisor `last_address` lookup missed under the stable key). **Fixed 2026-10-19:** `remember_address_for(address, device_id)` keys by identity when known; `rekey_profile`/`adopt_profile_identity` re-key provisional address ghosts onto the stable id at identity-learning sites (`learn_device_id`, `get_device_info`) and persist; same-host stale ghosts are pruned. Live-verified: both phones keyed by `ro.serialno` with current `last_address` after migration+learn.

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

### [~] M9-T9 Multi-device QA
- Run the TESTING.md multi-device rows: 3 phones streaming logs; drop/reconnect one phone; actions always hit the targeted phone; resource limits hold.
- Cross-check that M11 wizard saves profiles per phone (hands off to M11-T2/T14 once implemented).
- **Live check:** two connected phones (CPH2239 Android 11 + SM-T225 Android 14) verified 2026-10-19 — see the live QA record below. The 3-phone streaming row still needs a third phone (the stub-based 3-phone row is recorded in M10-T5).

## Exit check

- [ ] Two or more phones appear as tabs; each keeps its own view, terminal sessions, and log buffer across switching and app restart. *(Tabs/sessions/buffers ✓ live; per-tab view memory now persists and restores across restart (fixed 2026-10-19, `hacc.tabRoutes` + `startupRoute`); direct tab-click still needs a human check because automation hits the tab's × child.)*
- [ ] Per-phone profiles (alias, color, Hermes/Termux overrides, auto-connect) persist and take effect via `effective_config`; legacy settings migrated. *(FAIL — profiles are address-keyed ghosts; see M9-T2 regression note.)*
- [ ] Wireless port change on one phone rebinds its tab, sessions, host key, and history to the same `device_id`. *(✓ live: wireless debugging toggled, connect port 33843→37809, same tab/identity, no duplicate.)*
- [ ] One phone offline/hung never blocks the other phones' status, streams, or reconnect attempts (verified with fakes and live). *(✓ live: SM drop/offline left CPH streaming uninterrupted; supervisor backoff ran 1..8 per phone.)*
- [ ] Overview shows a live card per phone. *(✓ live: battery/storage/gateway per card, Open switches tabs.)*
- [ ] Every destructive confirmation names the target phone; broadcast is limited to predefined actions with multi-select. *(✓ live: "Close SM T225 (SM_T225)?" dialog; disconnect dialog content covered by unit tests.)*
- [ ] M11-T2/T14 can persist per-phone HermesConfig (dependency verified). *(Blocked by the profile-keying regression above.)*

## Live QA record (2026-10-19)

- **Setup:** packaged release build from `2c2b8d3`; OnePlus CPH2239T2 (Android 11, `192.168.0.107:46351`) and Samsung SM-T225 (Android 14, wireless debugging) — four adb transports (IP + mDNS alias per phone) rendered as exactly **2 tabs** with correct per-device info (serial via `ro.serialno`, Android 14/SDK 34, Termux installed vs not).
- **Passing rows:** tab strip + dedupe; per-device log buffers (0 lines vs live stream, then 1.5k vs 6k concurrently); log stream survives navigation and keeps buffering; terminal targeting (`echo TARGET:$(getprop ro.serialno)` on CPH2239's tab → `TARGET:<ro.serialno>` (the correct OnePlus serial), exit 0); Overview live cards; close-tab confirmation names the phone; SM drop left CPH streaming 2,170→6,290+ lines uninterrupted; reconnect supervisor engaged within 1 s of Wi-Fi loss with schedule 1/2/5/10/30 s and recovered after Wi-Fi returned; wireless-debugging toggle → new port → same tab rebind; tab order + last-selected tab restored after restart; Connect form validates and attaches (recovered the phone after finding 2).
- **Failures/regressions:** profile migration keying (finding 1), alias-loss auto-recovery + Retry (finding 2 — supervisor exhausted 8/8 and Retry stayed Disconnected while `adb devices` listed the phone under its IP serial; explicit Connect succeeded), stale log stream after serial change (finding 3), per-tab route memory not persisted across restart (finding 7). Tray counted transports (finding 6).
- **Not verifiable by automation:** direct tab-click select behavior — WebKit AXPress on `role="tab"` dispatches to the × child button, and backgrounded pixel clicks no-op on this WKWebView; awaiting a human click check.
- **Deferred:** M11 wizard profile-handoff cross-check (Wave 3), 3-phone streaming row (needs a third phone).
