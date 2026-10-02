# Chat Session Access Spike

**Status:** Hermes REST API selected for the M13 read path; live-device verification still required.
**Checked:** 2026-10-03 against the current Hermes Agent CLI reference and Web Dashboard documentation.

## Findings

- Hermes documents a loopback REST API served by `hermes serve` / `hermes dashboard` (default port 9119). Loopback binds do not engage the dashboard's remote-auth gate; this app must not change the bind to a LAN interface.
- `GET /api/sessions` returns the 20 most recent sessions with metadata and preview. The documented response includes model, token counts, timestamps, and a conversation preview. The Web Dashboard's Sessions page also exposes source, title, and message count.
- `GET /api/sessions/{session_id}` returns single-session metadata.
- `GET /api/sessions/{session_id}/messages` returns bounded pages of messages, including tool calls and timestamps. Default page size is 500, maximum is 500; `limit`, `offset`, and `order=oldest|latest` are supported.
- `GET /api/sessions/search?q=...` searches message content and returns matching session IDs/snippets.
- `hermes chat --resume <session_id>` resumes a session by ID, including sessions exposed by Hermes' session store. Chat's `--format stream-json` remains the existing streaming path.
- Hermes' CLI offers `hermes sessions export` as JSONL, but `hermes sessions list` is documented as a human-readable table. The dashboard REST API is therefore preferred over parsing terminal output or exporting all sessions just to populate previews.
- The dashboard API is documented but requires the Hermes web extra. Standard PM `all` setup includes that extra; installations missing it must receive a clear unsupported/unavailable error rather than falling back to a database read.

## M13 implementation decision

Use the dashboard REST API from inside the selected Hermes environment, through the existing authenticated Termux SSH transport. Start/reuse `hermes serve` bound only to `127.0.0.1`; issue API requests to its loopback listener from that same environment. No new ADB port forward is needed, and the service must never bind to Wi-Fi/LAN.

The Hermes store remains authoritative. The app parses only documented REST responses, requests message pages on demand, and persists no transcript content. Session identity is scoped by stable device ID plus configured Hermes environment/home.

## Compatibility checks still required

- Verify the exact JSON field names and error envelopes against a connected Termux/proot install before treating DTOs as stable.
- Verify whether `hermes serve --skip-build` is available and sufficient for API-only use; do not trigger a web UI build or open a browser as a side effect.
- Verify server startup/readiness, already-running detection, and behavior when another process owns port 9119.
- Verify a gateway-created session can be opened and resumed through `hermes chat --resume` without changing its source/platform identity.

## References

- [Hermes Web Dashboard REST API](https://hermes-agent.nousresearch.com/docs/user-guide/features/web-dashboard#rest-api)
- [Hermes CLI session commands](https://hermes-agent.nousresearch.com/docs/reference/cli-commands#hermes-sessions)
- [Hermes session storage and resume](https://hermes-agent.nousresearch.com/docs/user-guide/sessions)