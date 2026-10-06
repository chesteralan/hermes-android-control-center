# M13 — Chat Session History (v1.2.0)

**Goal:** Browse previous Hermes conversations, inspect previews/transcripts, and continue a selected session from the app. **Depends on:** M12. **Source of truth:** Hermes' existing session store on the phone; the app does not create a second canonical transcript database.

## Scope

- List sessions from the selected phone's configured Hermes environment, including sessions created by the app's Chat view and by enabled gateway platforms.
- Show source, title (or a useful fallback), last activity, and a short conversation preview.
- Open and review a conversation, then continue it by resuming its existing Hermes session ID. Stream the next response in the existing Chat view.
- Keep selection and navigation device-scoped. A session from one phone or Hermes environment must never be resumed against another.
- Persist only lightweight UI state (such as the selected session ID and list filters) in the desktop app. Keep transcripts and session metadata authoritative on the phone.
- Treat session content as sensitive: do not log prompts, previews, transcripts, or tool output in Rust tracing or frontend diagnostics.

## Non-goals

- Creating an app-owned database or mirroring complete transcripts locally.
- Deleting, archiving, renaming, or editing Hermes sessions.
- Managing Hermes profiles that are not selected by the current device/environment configuration.
- Replacing Hermes' session format or changing gateway session behavior.

## Tasks

### [~] M13-T1 Hermes session interface spike
- Verified the documented loopback REST API: recent session metadata/previews, metadata by ID, paginated messages (maximum 500), and full-text search.
- Selected the Hermes REST API served by `hermes serve`/`hermes dashboard`, called from inside the configured Hermes environment over the existing SSH bridge. The API stays bound to 127.0.0.1; no extra ADB forward or LAN listener is needed.
- Hermes CLI `sessions list` is human-readable, so the app will not parse its table. `sessions export` JSONL remains a fallback for transcript diagnosis, not the session-list path. The app will not read Hermes' internal SQLite schema.
- Documented source references, dependencies, bounds, and remaining compatibility checks in [spikes/chat-sessions.md](../spikes/chat-sessions.md).
- **Local test:** an unrelated HTTP service on the loopback API port is rejected without spawning a duplicate Hermes service.
- **Remaining live checks:** verify port-conflict handling on-device and resume a gateway-created session on a supported minimum/current Hermes version.

### [x] M13-T2 Typed Rust session access
- Add Rust session DTOs and commands to list/filter sessions and retrieve a bounded transcript through the existing Termux SSH bridge and Hermes loopback REST API.
- Ensure/reuse the API server bound to `127.0.0.1` without opening the browser or building the web frontend; report missing web dependencies and occupied ports clearly.
- Scope every request to the target serial and configured Hermes environment/home. Use Hermes' supported profile selection where applicable; never infer a profile from a session ID alone.
- Parse structured output with a JSON/JSONL parser, enforce response/message-size limits, and return explicit unsupported-version, unavailable, and malformed-data errors.
- Do not include session content in logs. Keep SSH command arguments shell-safe.
- **Live verification:** On one connected Hermes v0.21.4 device, the configured home and owner token verified, and the session-list/transcript-page schema matched the DTO parser. Cold `/api/status` exceeded 2 seconds, so the bounded inner HTTP timeout is now 10 seconds.
- **Implementation note:** Raw API JSON parsing maps malformed or truncated responses to a clear configuration error; the truncation case is covered by a regression test.
- **Tests:** fixtures for structured list/transcript output, multiple sources, missing optional fields, malformed/truncated output, limits, and safe session-ID handling.

### [x] M13-T3 Session browser and previews
- Add a session list alongside the existing Chat view with loading, empty, refresh, error, and device-offline states.
- Show title or first-user-message fallback, latest user/assistant preview, source badge, and last activity; truncate previews without changing the stored content.
- Support text search and source filtering when supported by the session interface; otherwise filter the bounded result set locally and label the scope clearly.
- Preserve stable ordering and selection while the list refreshes. Do not silently switch the active phone or session.
- **Implementation note:** Text and source filters operate locally on the loaded recent-session set, and the sidebar labels that scope. Transient filters reset when the device/environment changes. UI tests cover source/text filtering, API errors, device isolation, and selection stability when refresh changes ordering.
- **Tests:** list rendering, source filtering, empty/error/offline states, stable selection, and device/environment isolation.

### [x] M13-T4 Transcript review and resume
- Load a selected transcript in the existing Chat view, preserving user/assistant turns and presenting tool activity as secondary/collapsible content.
- Continue the selected session with the existing Hermes session ID; appended turns must preserve the loaded history and use the current streaming/cancellation behavior.
- Keep the composer available during a response; queue additional prompts per device/environment in FIFO order, show pending prompts, allow removal, and start the next prompt after completion or cancellation using the latest session ID.
- Provide an explicit New Chat action that starts without a resume ID and does not alter the previous session.
- Update selection if Hermes returns a rotated session ID after compression; keep the old session reachable if Hermes exposes it as a distinct lineage entry.
- **Implementation note:** A rotated session ID from Hermes updates the device/environment-scoped selected-session preference. Additional prompts stay queued until the current response completes or is stopped, then drain in order with the latest session ID.
- **Tests:** transcript hydration, resume args, stream append, new-session isolation, compression/session-ID update, queue FIFO/removal/cancel-advance, cancellation, and failed resume.

### [x] M13-T5 Device-scoped restoration and documentation
- Persist the selected session ID and non-sensitive list preferences per stable `device_id` and configured Hermes environment/home. Do not persist transcripts or preview text by default.
- If the phone is unavailable, retain the selection and show an explicit reconnect state; do not fabricate cached session data.
- Document session sources, environment/profile scope, privacy behavior, supported Hermes version/interface, and troubleshooting.
- **Implementation note:** The selected session ID is stored per device/environment/home and its transcript is reloaded from Hermes. Free-text search, previews, and transcript content are not persisted because search terms may contain session content.
- **Documentation:** Session source labels, device/environment/home scope, local persistence and privacy behavior, profile limitations, the v0.21.4-only live verification, and troubleshooting are documented in [TROUBLESHOOTING.md](../guides/TROUBLESHOOTING.md#chat-sessions) and [the access spike](../spikes/chat-sessions.md). No minimum supported Hermes version is claimed.
- **Tests:** app restart restoration, wireless serial change with stable device ID, environment separation, and unavailable-device behavior.

### [~] M13-T6 Cross-platform and performance QA
- Verify list/transcript flows on macOS, Windows, and Linux after M12, including cancellation and reconnect during retrieval.
- Bound list page size, transcript size, and render work; virtualize long transcripts if needed.
- Run the normal milestone CI gates and add a manual connected-phone check against the minimum and current supported Hermes versions.
- **Implementation note:** Rust caps session-list response bodies at 2 MiB, transcript response bodies at 8 MiB, and clamps each message page to 1–500 entries. Regression tests cover page-size clamping and oversized-response rejection. The Chat view virtualizes transcripts over 100 messages; a 250-message viewport-slice test passes. Cross-platform and connected-phone/version-matrix performance checks remain open.

## Exit check

- [ ] The session browser shows app-created and gateway-created sessions from the selected Hermes environment with useful previews.
- [x] A user can open an old session and continue it in Chat without losing the existing transcript or streaming behavior.
- [x] Session selection survives app restart and wireless serial changes without crossing devices/environments.
- [x] No second canonical session database exists; transcript content is not logged or persisted by the app.
- [x] Session-list and transcript limits are documented and tested.
- [ ] Supported Hermes versions are documented and tested across the supported range.
- [ ] All M12-supported desktop platforms pass CI and manual connected-phone QA.