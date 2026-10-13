# Chat Session Access Spike

**Status:** Hermes REST API selected for the M13 read path; session-list and transcript-page reads verified on one connected device running Hermes Agent v0.21.4. Gateway-session resume and the supported version range remain unverified.
**Checked:** 2026-10-09 against the Hermes Agent CLI reference, Web Dashboard REST API documentation, upstream [releases](https://github.com/NousResearch/hermes-agent/releases), plus one live device.

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

The Hermes store remains authoritative. The app parses only documented REST responses, requests message pages on demand, and persists no transcript content. Session identity is scoped by stable device ID plus configured Hermes environment/home. The desktop stores only the selected session ID for restoration; it does not persist message content, previews, or free-text search terms, which may themselves contain sensitive session content.

The Chat UI has no Hermes profile selector; requests use the session API for the configured environment/home, and saved selection is not keyed by a profile inferred from the session ID. Non-default profile selection and isolation have not been verified and are not claimed as supported.

The app requests the 20-session recent list and clamps transcript pages to 1–500 messages (the Hermes API maximum). It rejects list response bodies over 2 MiB and transcript-page bodies over 8 MiB. These request bounds have regression coverage in `src-tauri/src/commands/sessions.rs`; they are client safeguards, not claims that Hermes will return a full 500-message page within the byte limit.

Live verification on Hermes Agent v0.21.4 confirmed the session-list `id` field, string message content with `role`, and a transcript `pagination` object containing `limit`, `offset`, `order`, and `returned`. The API health, configured home, owner token, session list, and one-message page all verified while bound to 127.0.0.1. Cold `/api/status` took 5.9–7.3 seconds, so the request helper timeout was raised from 2 to 10 seconds. The temporary server was stopped after each probe.

## Compatibility checks still required

- The upstream release page lists v0.21.6 as the latest release as of 2026-10-09. This establishes the current published version only, not REST API compatibility across releases. No minimum Hermes version is claimed; the live check covers v0.21.4 only, so the minimum/current supported version matrix remains open.
- Verify documented JSON fields and error envelopes against the minimum supported Hermes version before treating DTOs as stable across versions.
- `hermes serve --skip-build`, loopback readiness, owner-token validation, and a bounded transcript page were verified on v0.21.4. Port-conflict behavior still needs live verification against an unrelated listener.
- The connected phone exposed CLI, cron, and oneshot sessions, but no gateway-created session; verify gateway-session browsing and resume without changing source/platform identity.

## References

- [Hermes Web Dashboard REST API](https://hermes-agent.nousresearch.com/docs/user-guide/features/web-dashboard#rest-api)
- [Hermes CLI session commands](https://hermes-agent.nousresearch.com/docs/reference/cli-commands#hermes-sessions)
- [Hermes session storage and resume](https://hermes-agent.nousresearch.com/docs/user-guide/sessions)