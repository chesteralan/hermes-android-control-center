# M8 — Hermes Control API (Phase 6)

**Goal:** Optional Termux-side service with structured endpoints; UI agnostic to transport. **Depends on:** M7. ADR-011.

## Termux service (`android/hermes-control/`)

### [x] M8-T1 Service skeleton

- Python ≥ 3.11, minimal deps (`aiohttp`), single package + `install.sh` + `hermes-control.toml` config.
- Config: bind host (default `127.0.0.1`), port (default 8765), Hermes commands (same semantics as `HermesConfig`), log command/path, token.
- Refuse to start on non-loopback bind without token.
- **Implementation note:** Added a Python package and aiohttp entrypoint, a Termux install script and sample config, and tested default loopback binding, non-loopback token enforcement, invalid ports, and Hermes defaults. The package installed in an isolated venv and its CLI entrypoint smoke test passed.

### [x] M8-T2 Endpoints

| Method | Path                        | Body                      | Response                                          |
| ------ | --------------------------- | ------------------------- | ------------------------------------------------- |
| GET    | `/health`                   | —                         | `{ ok, version }`                                 |
| GET    | `/status`                   | —                         | `HermesStatus` JSON (same schema as Rust)         |
| POST   | `/command`                  | `{ command, timeout_ms }` | `CommandResult`                                   |
| POST   | `/start` `/stop` `/restart` | —                         | `{ result: CommandResult, status: HermesStatus }` |
| WS     | `/logs?tail=200`            | —                         | stream of `LogLine` JSON                          |
| WS     | `/command/stream`           | `{ command }`             | stream of `StreamEvent` JSON                      |

- Auth: `Authorization: Bearer <token>` when token configured (constant-time compare).
- `/command` disabled unless `allow_arbitrary_commands = true`.
- **Implementation note:** Added authenticated HTTP handlers, typed response envelopes, bounded command timeouts, process-group cleanup on timeout/disconnect, tail/follow log WebSocket, and streamed command events. The service reports Hermes and runit supervisor state from Termux-visible `/proc` and supervisor files.

### [x] M8-T3 Service tests

- pytest for each endpoint, auth, bind refusal, log tail.
- **Implementation note:** 20 pytest cases cover auth, all endpoint families, command policy, action overlay, log tailing, both proot rootfs layouts, and command process cleanup.

### [x] M8-T4 JSON schema contract

- Export JSON Schema from Rust types (`schemars`) to `android/hermes-control/schema/`; Python tests validate responses against it.
- **Implementation note:** `npm run gen:schemas` exports Rust `HermesStatus`, `CommandResult`, `StreamEvent`, `LogLine`, and `ErrorPayload` schemas; Python endpoint tests validate responses with `jsonschema`.

### [x] M8-T5 Process supervision

- `termux-services` runit script (`sv up hermes-control`); docs for `termux-wake-lock`.
- **Implementation note:** The installer adds the runit script idempotently, installs `termux-services` if needed, and the app explicitly starts Termux's `service-daemon` with `SVDIR` set for non-interactive SSH. The service README documents `sv`, wake locks, and Termux:Boot.

## Desktop

### [x] M8-T6 `ApiTransport`

- `reqwest` + `tokio-tungstenite` over `adb forward tcp:0 tcp:8765`; token from Keychain.
- Implements `DeviceTransport` and `HermesManager` fast-path (structured `/status`).
- **Implementation note:** Added HTTP/WebSocket transport over the OS-assigned ADB forward, Apple-native Keychain tokens scoped by stable device ID, typed status/action fast paths, and selected-transport routing for terminal, chat, sessions, Hermes tools, and status/actions.

### [x] M8-T7 `ApiWsLogSource`

- Implements `LogSource`; reconnect with backoff.
- **Implementation note:** Added a cancellable API WebSocket log source with ping handling and capped exponential reconnect. Gateway logs use the API; unsupported log sources require explicit SSH fallback.

### [x] M8-T8 Install/upgrade from app

- Over Termux SSH: upload service, run `install.sh`, enable service, generate token, store in Keychain. Shows diff/version before upgrade.
- **Implementation note:** Settings previews installed/target versions and files, uploads embedded service assets over SSH, writes Hermes action/environment settings without replacing the user's TOML, refreshes runit after token rotation, and stores a generated 256-bit token in Keychain and a mode-600 Termux token file. The service CLI accepts the string config path passed by argparse.

### [x] M8-T9 Transport switch in Settings

- Hermes transport: Termux SSH | Control API; "Test" button; fallback to SSH only if user enabled "fallback" — otherwise surface error.
- **Implementation note:** Added the transport selector, explicit SSH fallback checkbox, install/upgrade review, and API test action. API failures surface unless fallback is enabled.

## Exit check

- [~] Verify Hermes start/stop/restart state transitions on a connected Android device; API Test, token auth, structured status without SSH fallback, and gateway-log streaming have been exercised.
- **QA note:** On the OPPO CPH2239, the QA app reached Control API 0.1.0 through ADB forwarding; unauthenticated `/health` returned 401. With SSH fallback disabled, Hermes status refreshed to the same stopped gateway state observed over SSH. The gateway-log WebSocket delivered its 200-line tail and was stopped cleanly. Start/stop/restart transitions remain untested to avoid starting a real Hermes gateway and its integrations.
