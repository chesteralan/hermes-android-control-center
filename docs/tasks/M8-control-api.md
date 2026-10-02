# M8 — Hermes Control API (Phase 6)

**Goal:** Optional Termux-side service with structured endpoints; UI agnostic to transport. **Depends on:** M7. ADR-011.

## Termux service (`android/hermes-control/`)

### [ ] M8-T1 Service skeleton
- Python ≥ 3.11, minimal deps (`aiohttp`), single package + `install.sh` + `hermes-control.toml` config.
- Config: bind host (default `127.0.0.1`), port (default 8765), Hermes commands (same semantics as `HermesConfig`), log command/path, token.
- Refuse to start on non-loopback bind without token.

### [ ] M8-T2 Endpoints
| Method | Path | Body | Response |
|---|---|---|---|
| GET | `/health` | — | `{ ok, version }` |
| GET | `/status` | — | `HermesStatus` JSON (same schema as Rust) |
| POST | `/command` | `{ command, timeout_ms }` | `CommandResult` |
| POST | `/start` `/stop` `/restart` | — | `{ result: CommandResult, status: HermesStatus }` |
| WS | `/logs?tail=200` | — | stream of `LogLine` JSON |
| WS | `/command/stream` | `{ command }` | stream of `StreamEvent` JSON |
- Auth: `Authorization: Bearer <token>` when token configured (constant-time compare).
- `/command` disabled unless `allow_arbitrary_commands = true`.

### [ ] M8-T3 Service tests
- pytest for each endpoint, auth, bind refusal, log tail.

### [ ] M8-T4 JSON schema contract
- Export JSON Schema from Rust types (`schemars`) to `android/hermes-control/schema/`; Python tests validate responses against it.

### [ ] M8-T5 Process supervision
- `termux-services` runit script (`sv up hermes-control`); docs for `termux-wake-lock`.

## Desktop

### [ ] M8-T6 `ApiTransport`
- `reqwest` + `tokio-tungstenite` over `adb forward tcp:0 tcp:8765`; token from Keychain.
- Implements `DeviceTransport` and `HermesManager` fast-path (structured `/status`).

### [ ] M8-T7 `ApiWsLogSource`
- Implements `LogSource`; reconnect with backoff.

### [ ] M8-T8 Install/upgrade from app
- Over Termux SSH: upload service, run `install.sh`, enable service, generate token, store in Keychain. Shows diff/version before upgrade.

### [ ] M8-T9 Transport switch in Settings
- Hermes transport: Termux SSH | Control API; "Test" button; fallback to SSH only if user enabled "fallback" — otherwise surface error.

## Exit check
- [ ] Switching transport changes no UI behavior; contract tests green on both sides; service listens on 127.0.0.1 only.
