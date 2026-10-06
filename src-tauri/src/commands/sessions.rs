use std::time::Duration;

use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::config::HermesConfig;
use crate::error::AppError;
use crate::state::AppState;
use crate::termux::shell_escape;
use crate::termux::ssh::TermuxSshTransport;
use crate::transport::DeviceTransport;

const DASHBOARD_PORT: u16 = 9119;
const API_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_LIST_BYTES: usize = 2 * 1024 * 1024;
const MAX_TRANSCRIPT_BYTES: usize = 8 * 1024 * 1024;
const MAX_PAGE_SIZE: u32 = 500;

const API_REQUEST_SCRIPT: &str = r#"
import errno, hashlib, hmac, json, os, stat, subprocess, sys, time, urllib.error, urllib.request

request = json.loads(sys.argv[1])
port = int(request["port"])
base = "http://127.0.0.1:{}".format(port)
timeout = 10
session_token = None

def read_session_token():
    expected_home = os.path.realpath(os.path.expanduser(request["hermes_home"]))
    status = get_json("/api/status", 1024 * 1024)
    server_home = status.get("hermes_home")
    if (
        status.get("auth_required")
        or not isinstance(server_home, str)
        or os.path.realpath(os.path.expanduser(server_home)) != expected_home
    ):
        raise RuntimeError("Hermes session API does not report the configured Hermes home in loopback mode")
    lock_dirs = []
    configured_dir = os.environ.get("HERMES_GATEWAY_LOCK_DIR")
    if configured_dir:
        lock_dirs.append(os.path.expanduser(configured_dir))
    state_home = os.environ.get("XDG_STATE_HOME")
    if state_home:
        lock_dirs.append(os.path.join(os.path.expanduser(state_home), "hermes", "gateway-locks"))
    lock_dirs.append(os.path.expanduser("~/.local/state/hermes/gateway-locks"))

    for lock_dir in dict.fromkeys(lock_dirs):
        try:
            record_path = os.path.join(lock_dir, "host-serve.json")
            token_path = os.path.join(lock_dir, "host-serve.token")
            if stat.S_IMODE(os.stat(token_path).st_mode) & 0o077:
                continue
            with open(record_path, encoding="utf-8") as record_file:
                record = json.load(record_file)
            with open(token_path, encoding="utf-8") as token_file:
                token = token_file.read(4096).strip()
            owner_home = record.get("home")
            if (
                not token
                or record.get("role") != "serve"
                or record.get("port") != port
                or (owner_home and os.path.realpath(os.path.expanduser(owner_home)) != expected_home)
            ):
                continue
            fingerprint = hashlib.sha256(token.encode("utf-8")).hexdigest()[:16]
            if hmac.compare_digest(str(record.get("tokenFingerprint", "")), fingerprint):
                identity = get_json("/api/host/identity", 65536, token)
                if identity.get("role") == "serve" and identity.get("pid") == record.get("pid"):
                    return token
        except (OSError, ValueError, TypeError):
            continue
    raise RuntimeError("Hermes session API is running, but its owner-only session token could not be verified")

def get_json(path, max_bytes, token=None):
    headers = {"Accept": "application/json"}
    if token or session_token:
        headers["X-Hermes-Session-Token"] = token or session_token
    req = urllib.request.Request(
        base + path,
        headers=headers,
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as response:
            body = response.read(max_bytes + 1)
    except urllib.error.HTTPError as exc:
        detail = exc.read(4096).decode("utf-8", errors="replace").strip()
        if exc.code == 401 and path != "/api/status":
            try:
                status = get_json("/api/status", 1024 * 1024)
                detail += " (auth_required={})".format(status.get("auth_required", "unknown"))
            except Exception:
                pass
        raise RuntimeError("Hermes API HTTP {}: {}".format(exc.code, detail or exc.reason))
    if len(body) > max_bytes:
        raise RuntimeError("Hermes session response exceeds the size limit")
    return json.loads(body.decode("utf-8"))

def server_ready():
    try:
        health = get_json("/api/health", 65536)
        if not isinstance(health, dict) or health.get("ok") is not True:
            raise RuntimeError("Hermes session API returned an invalid health response")
        return True
    except urllib.error.URLError as exc:
        if isinstance(exc.reason, OSError) and exc.reason.errno == errno.ECONNREFUSED:
            return False
        raise RuntimeError("Hermes session API health probe failed; refusing to launch a duplicate server") from exc
    except OSError as exc:
        if exc.errno == errno.ECONNREFUSED:
            return False
        raise RuntimeError("Hermes session API health probe failed; refusing to launch a duplicate server") from exc

def ensure_server():
    if server_ready():
        return

    env = os.environ.copy()
    home = os.path.expanduser(request["hermes_home"])
    env["HERMES_HOME"] = home
    log_dir = os.path.join(home, "logs")
    os.makedirs(log_dir, exist_ok=True)
    log = open(os.path.join(log_dir, "hacc-session-api.log"), "ab", buffering=0)
    try:
        process = subprocess.Popen(
            ["hermes", "serve", "--host", "127.0.0.1", "--port", str(port), "--skip-build"],
            stdin=subprocess.DEVNULL,
            stdout=log,
            stderr=subprocess.STDOUT,
            env=env,
            start_new_session=True,
            close_fds=True,
        )
    except OSError as exc:
        log.close()
        raise RuntimeError("Could not start the Hermes session API: {}".format(exc))
    log.close()

    deadline = time.monotonic() + 40
    while time.monotonic() < deadline:
        if server_ready():
            return
        if process.poll() is not None:
            raise RuntimeError("Hermes session API exited during startup; check {}/logs/hacc-session-api.log".format(home))
        time.sleep(0.25)
    raise RuntimeError("Timed out starting the Hermes session API on loopback port {}".format(port))

try:
    ensure_server()
    session_token = read_session_token()
    result = get_json(request["path"], request["max_bytes"])
    sys.stdout.write(json.dumps(result, ensure_ascii=False, separators=(",", ":")))
except Exception as exc:
    sys.stderr.write(str(exc) + "\n")
    sys.exit(1)
"#;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HermesSessionSummary {
    pub session_id: String,
    pub title: Option<String>,
    pub source: Option<String>,
    pub model: Option<String>,
    #[ts(type = "number | null")]
    pub message_count: Option<u64>,
    pub last_active: Option<String>,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HermesSessionMessage {
    pub id: Option<String>,
    pub role: String,
    pub content: String,
    pub timestamp: Option<String>,
    pub tool_name: Option<String>,
    pub tool_calls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HermesSessionPage {
    pub session_id: String,
    pub messages: Vec<HermesSessionMessage>,
    pub offset: u32,
    pub limit: u32,
    #[ts(type = "number | null")]
    pub total: Option<u64>,
    pub has_more: bool,
}

#[derive(Serialize)]
struct ApiRequest<'a> {
    path: &'a str,
    port: u16,
    hermes_home: &'a str,
    max_bytes: usize,
}

fn build_api_command(cfg: &HermesConfig, path: &str, max_bytes: usize) -> String {
    let request = serde_json::to_string(&ApiRequest {
        path,
        port: DASHBOARD_PORT,
        hermes_home: &cfg.hermes_home,
        max_bytes,
    })
    .expect("session API request serializes");
    let command = format!(
        "python3 -c {} {}",
        shell_escape(API_REQUEST_SCRIPT),
        shell_escape(&request)
    );
    crate::hermes::env::wrap(cfg, &command)
}

fn valid_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id.len() <= 128
        && session_id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
}

fn value_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn first_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(value_string))
}

fn first_u64(value: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
    })
}

fn array_field<'a>(
    value: &'a serde_json::Value,
    keys: &[&str],
) -> Option<&'a Vec<serde_json::Value>> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(serde_json::Value::as_array))
}

fn parse_sessions(value: &serde_json::Value) -> Result<Vec<HermesSessionSummary>, AppError> {
    let sessions = value
        .as_array()
        .or_else(|| array_field(value, &["sessions", "items", "results"]))
        .ok_or_else(|| {
            AppError::Config("Hermes returned an unsupported session-list response.".into())
        })?;

    Ok(sessions
        .iter()
        .filter_map(|session| {
            let session_id = first_string(session, &["session_id", "sessionId", "id"])?;
            Some(HermesSessionSummary {
                session_id,
                title: first_string(session, &["title"]),
                source: first_string(session, &["source", "platform"]),
                model: first_string(session, &["model", "model_name", "modelName"]),
                message_count: first_u64(
                    session,
                    &["message_count", "messageCount", "messages_count"],
                ),
                last_active: first_string(
                    session,
                    &[
                        "last_active",
                        "lastActive",
                        "last_active_at",
                        "updated_at",
                        "updatedAt",
                        "started_at",
                    ],
                ),
                preview: first_string(
                    session,
                    &["preview", "snippet", "last_message", "lastMessage"],
                ),
            })
        })
        .collect())
}

fn content_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| part.get("content").and_then(serde_json::Value::as_str))
            })
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn parse_messages(
    session_id: &str,
    value: &serde_json::Value,
    offset: u32,
    limit: u32,
) -> Result<HermesSessionPage, AppError> {
    let messages = value
        .as_array()
        .or_else(|| array_field(value, &["messages", "items", "results"]))
        .ok_or_else(|| {
            AppError::Config("Hermes returned an unsupported transcript response.".into())
        })?;
    let pagination = value.get("pagination").unwrap_or(value);
    let total = first_u64(
        value,
        &["total", "total_count", "totalCount", "message_count"],
    )
    .or_else(|| {
        first_u64(
            pagination,
            &["total", "total_count", "totalCount", "message_count"],
        )
    });
    let parsed = messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            let role = first_string(message, &["role", "type"])?;
            let tool_name = first_string(message, &["tool_name", "toolName", "name"]);
            let tool_calls = message
                .get("tool_calls")
                .or_else(|| message.get("toolCalls"))
                .and_then(serde_json::Value::as_array)
                .map(|calls| {
                    calls
                        .iter()
                        .filter_map(|call| {
                            call.get("function")
                                .and_then(|function| function.get("name"))
                                .and_then(serde_json::Value::as_str)
                                .or_else(|| call.get("name").and_then(serde_json::Value::as_str))
                                .map(str::to_string)
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(HermesSessionMessage {
                id: first_string(message, &["id", "message_id", "messageId"])
                    .or_else(|| Some(format!("{}:{}", offset, index))),
                content: message
                    .get("content")
                    .map(content_text)
                    .or_else(|| first_string(message, &["output", "text"]))
                    .unwrap_or_default(),
                role,
                timestamp: first_string(message, &["timestamp", "created_at", "createdAt"]),
                tool_name,
                tool_calls,
            })
        })
        .collect::<Vec<_>>();
    let reported_more = value
        .get("has_more")
        .or_else(|| value.get("hasMore"))
        .or_else(|| pagination.get("has_more"))
        .or_else(|| pagination.get("hasMore"))
        .and_then(serde_json::Value::as_bool);
    let returned = first_u64(pagination, &["returned"]).unwrap_or(parsed.len() as u64);
    let has_more = reported_more.unwrap_or_else(|| {
        total.is_some_and(|count| offset as u64 + returned < count) || returned >= limit as u64
    });

    Ok(HermesSessionPage {
        session_id: session_id.to_string(),
        messages: parsed,
        offset,
        limit,
        total,
        has_more,
    })
}

async fn request_api(
    state: &AppState,
    serial: &str,
    cfg: &HermesConfig,
    path: &str,
    max_bytes: usize,
) -> Result<serde_json::Value, AppError> {
    let transport: TermuxSshTransport = state.termux_transport(serial).await?;
    let command = build_api_command(cfg, path, max_bytes);
    let result = transport.execute(&command, API_TIMEOUT).await?;
    if result.exit_code != Some(0) {
        let diagnostics = [result.stderr.trim(), result.stdout.trim()]
            .into_iter()
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        return Err(AppError::CommandFailed {
            command: "Hermes session API request".into(),
            exit_code: result.exit_code,
            stderr: if diagnostics.is_empty() {
                format!(
                    "No diagnostics were returned. Check {}/logs/hacc-session-api.log on the phone.",
                    cfg.hermes_home.trim_end_matches('/')
                )
            } else {
                diagnostics
            },
        });
    }
    parse_api_response(&result.stdout)
}

fn parse_api_response(body: &str) -> Result<serde_json::Value, AppError> {
    serde_json::from_str(body)
        .map_err(|error| AppError::Config(format!("Hermes returned invalid session JSON: {error}")))
}

#[tauri::command]
pub async fn list_hermes_sessions(
    state: State<'_, AppState>,
    serial: String,
) -> Result<Vec<HermesSessionSummary>, AppError> {
    let cfg = state.config.read().await.hermes.clone();
    let value = request_api(&state, &serial, &cfg, "/api/sessions", MAX_LIST_BYTES).await?;
    parse_sessions(&value)
}

fn session_messages_path(
    session_id: &str,
    offset: Option<u32>,
    limit: Option<u32>,
) -> (String, u32) {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(MAX_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE);
    (
        format!("/api/sessions/{session_id}/messages?limit={limit}&offset={offset}&order=oldest"),
        limit,
    )
}

#[tauri::command]
pub async fn get_hermes_session_messages(
    state: State<'_, AppState>,
    serial: String,
    session_id: String,
    offset: Option<u32>,
    limit: Option<u32>,
) -> Result<HermesSessionPage, AppError> {
    if !valid_session_id(&session_id) {
        return Err(AppError::Config("Hermes session ID is invalid.".into()));
    }
    let offset = offset.unwrap_or(0);
    let (path, limit) = session_messages_path(&session_id, Some(offset), limit);
    let cfg = state.config.read().await.hermes.clone();
    let value = request_api(&state, &serial, &cfg, &path, MAX_TRANSCRIPT_BYTES).await?;
    parse_messages(&session_id, &value, offset, limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HermesEnvironment;

    #[test]
    fn health_probe_rejects_an_unrelated_listener_without_spawning() {
        let helper = API_REQUEST_SCRIPT
            .split("\ntry:\n    ensure_server()")
            .next()
            .unwrap();
        let checks = r#"
import io
def forbid_spawn(*args, **kwargs):
    raise AssertionError('spawned Hermes beside an unrelated listener')
subprocess.Popen = forbid_spawn
def get_json(path, max_bytes, token=None):
    assert path == '/api/health'
    raise urllib.error.HTTPError(
        'http://127.0.0.1:9119/api/health', 404, 'Not Found', {}, io.BytesIO(b'not Hermes')
    )
try:
    ensure_server()
    raise AssertionError('accepted an unrelated listener')
except RuntimeError as error:
    assert str(error) == 'Hermes session API health probe failed; refusing to launch a duplicate server'
"#;
        let output = std::process::Command::new("python3")
            .args([
                "-c",
                &format!("{helper}\n{checks}"),
                r#"{"port":9119,"hermes_home":"~/.hermes"}"#,
            ])
            .output()
            .expect("python3 runs the unrelated-listener regression check");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn verifies_tokens_with_empty_record_home_against_live_server() {
        let helper = API_REQUEST_SCRIPT
            .split("\ntry:\n    ensure_server()")
            .next()
            .unwrap();
        let checks = r#"
import tempfile
with tempfile.TemporaryDirectory() as directory:
    os.environ['HERMES_GATEWAY_LOCK_DIR'] = directory
    request['hermes_home'] = directory
    token = 'test-session-credential'
    record = {'home': '', 'role': 'serve', 'port': port, 'pid': 123,
              'tokenFingerprint': hashlib.sha256(token.encode()).hexdigest()[:16]}
    with open(os.path.join(directory, 'host-serve.json'), 'w') as handle:
        json.dump(record, handle)
    token_path = os.path.join(directory, 'host-serve.token')
    with open(token_path, 'w') as handle:
        handle.write(token)
    os.chmod(token_path, 0o600)
    def get_json(path, max_bytes, credential=None):
        if path == '/api/status':
            return {'auth_required': False, 'hermes_home': request['hermes_home']}
        assert credential == token
        return {'role': 'serve', 'pid': 123}
    assert read_session_token() == token
    record['pid'] = 456
    with open(os.path.join(directory, 'host-serve.json'), 'w') as handle:
        json.dump(record, handle)
    try:
        read_session_token()
        raise AssertionError('accepted a different live owner')
    except RuntimeError:
        pass
    request['hermes_home'] = directory + '/wrong-home'
    record['pid'] = 123
    record['home'] = directory
    with open(os.path.join(directory, 'host-serve.json'), 'w') as handle:
        json.dump(record, handle)
    try:
        read_session_token()
        raise AssertionError('accepted a conflicting record home')
    except RuntimeError:
        pass
"#;
        let output = std::process::Command::new("python3")
            .args([
                "-c",
                &format!("{helper}\n{checks}"),
                r#"{"port":9119,"hermes_home":"~/.hermes"}"#,
            ])
            .output()
            .expect("python3 runs the session helper regression check");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn health_probe_does_not_spawn_on_timeout_or_reject_a_ready_competing_server() {
        let helper = API_REQUEST_SCRIPT
            .split("\ntry:\n    ensure_server()")
            .next()
            .unwrap();
        let checks = r#"
import tempfile
def forbid_spawn(*args, **kwargs):
    raise AssertionError('spawned a duplicate server after a timeout')
subprocess.Popen = forbid_spawn
for failure in (TimeoutError('slow response'), urllib.error.URLError(TimeoutError('slow response'))):
    def get_json(path, max_bytes, token=None):
        assert path == '/api/health'
        raise failure
    try:
        ensure_server()
        raise AssertionError('accepted a timed-out health probe')
    except RuntimeError:
        pass
with tempfile.TemporaryDirectory() as directory:
    request['hermes_home'] = directory
    probes = []
    def get_json(path, max_bytes, token=None):
        assert path == '/api/health'
        probes.append(path)
        if len(probes) == 1:
            raise urllib.error.URLError(ConnectionRefusedError(errno.ECONNREFUSED, 'not started'))
        return {'ok': True}
    class ExitedProcess:
        def poll(self):
            raise AssertionError('checked child exit before the ready server')
    subprocess.Popen = lambda *args, **kwargs: ExitedProcess()
    ensure_server()
    assert len(probes) == 2
"#;
        let output = std::process::Command::new("python3")
            .args([
                "-c",
                &format!("{helper}\n{checks}"),
                r#"{"port":9119,"hermes_home":"~/.hermes"}"#,
            ])
            .output()
            .expect("python3 runs the startup regression check");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn builds_loopback_session_api_request_without_shell_interpolation() {
        let cfg = HermesConfig {
            environment: HermesEnvironment::Termux,
            path_prepend: vec![],
            hermes_home: "~/.hermes".into(),
            ..Default::default()
        };
        let command = build_api_command(
            &cfg,
            "/api/sessions/20261003_120000_a1b2c3/messages?limit=500&offset=0&order=oldest",
            MAX_TRANSCRIPT_BYTES,
        );
        assert!(command.contains("127.0.0.1"));
        assert!(API_REQUEST_SCRIPT.contains("timeout = 10"));
        assert!(command.contains("\"hermes\", \"serve\""));
        assert!(command.contains("X-Hermes-Session-Token"));
        assert!(command.contains("tokenFingerprint"));
        assert!(command.contains("20261003_120000_a1b2c3"));
        assert!(command.contains(r#""max_bytes":8388608"#));

        let list_command = build_api_command(&cfg, "/api/sessions", MAX_LIST_BYTES);
        assert!(list_command.contains(r#""max_bytes":2097152"#));
    }

    #[test]
    fn clamps_session_message_page_size() {
        assert_eq!(
            session_messages_path("s1", None, None),
            (
                "/api/sessions/s1/messages?limit=500&offset=0&order=oldest".into(),
                500
            )
        );
        assert_eq!(
            session_messages_path("s1", Some(12), Some(0)),
            (
                "/api/sessions/s1/messages?limit=1&offset=12&order=oldest".into(),
                1
            )
        );
        assert_eq!(
            session_messages_path("s1", Some(12), Some(900)),
            (
                "/api/sessions/s1/messages?limit=500&offset=12&order=oldest".into(),
                500
            )
        );
    }

    #[test]
    fn rejects_session_api_response_over_the_configured_size_limit() {
        let helper = API_REQUEST_SCRIPT
            .split("\ntry:\n    ensure_server()")
            .next()
            .unwrap();
        let checks = r#"
class OversizedResponse:
    def __enter__(self):
        return self
    def __exit__(self, *args):
        return False
    def read(self, size):
        assert size == 5
        return b'123456'
urllib.request.urlopen = lambda request, timeout: OversizedResponse()
try:
    get_json('/api/sessions', 4)
    raise AssertionError('accepted an oversized response')
except RuntimeError as error:
    assert str(error) == 'Hermes session response exceeds the size limit'
"#;
        let output = std::process::Command::new("python3")
            .args(["-c", &format!("{helper}\n{checks}"), r#"{"port":9119}"#])
            .output()
            .expect("python3 runs the response-size regression check");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn normalizes_session_list_aliases_and_optional_fields() {
        let parsed = parse_sessions(&serde_json::json!({
            "sessions": [{
                "id": "s1",
                "title": "Example",
                "platform": "telegram",
                "model_name": "model-x",
                "message_count": 4,
                "updated_at": "2026-10-03T12:00:00Z",
                "preview": "hello"
            }, { "session_id": "s2" }]
        }))
        .unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].source.as_deref(), Some("telegram"));
        assert_eq!(parsed[0].message_count, Some(4));
        assert_eq!(parsed[1].title, None);
    }

    #[test]
    fn parses_bounded_transcript_and_detects_more_pages() {
        let page = parse_messages(
            "s1",
            &serde_json::json!({
                "total": 3,
                "messages": [
                    { "id": "m1", "role": "user", "content": "hello" },
                    { "role": "assistant", "content": [{"type":"text", "text":"hi"}], "tool_calls": [{"function":{"name":"web_search"}}] }
                ]
            }),
            0,
            2,
        )
        .unwrap();
        assert_eq!(page.messages[1].content, "hi");
        assert_eq!(page.messages[1].tool_calls, vec!["web_search"]);
        assert!(page.has_more);
    }

    #[test]
    fn parses_hermes_serve_transcripts_with_nested_pagination() {
        let full_page = parse_messages(
            "s1",
            &serde_json::json!({
                "session_id": "s1",
                "profile": "default",
                "pagination": { "limit": 2, "offset": 0, "order": "oldest", "returned": 2 },
                "messages": [
                    { "id": "m1", "session_id": "s1", "role": "user", "content": "hello" },
                    { "id": "m2", "session_id": "s1", "role": "assistant", "content": "hi" }
                ]
            }),
            0,
            2,
        )
        .unwrap();
        assert_eq!(full_page.messages.len(), 2);
        assert!(full_page.has_more);

        let final_page = parse_messages(
            "s1",
            &serde_json::json!({
                "session_id": "s1",
                "pagination": { "limit": 2, "offset": 2, "order": "oldest", "returned": 1 },
                "messages": [
                    { "id": "m3", "session_id": "s1", "role": "assistant", "content": "done" }
                ]
            }),
            2,
            2,
        )
        .unwrap();
        assert_eq!(final_page.messages.len(), 1);
        assert!(!final_page.has_more);
    }

    #[test]
    fn rejects_unsafe_session_ids_and_unknown_payload_shapes() {
        assert!(!valid_session_id("../../state.db"));
        assert!(parse_sessions(&serde_json::json!({"unexpected": []})).is_err());
        assert!(parse_messages("s", &serde_json::json!({"bad": true}), 0, 10).is_err());
    }

    #[test]
    fn rejects_truncated_session_api_json() {
        assert!(matches!(
            parse_api_response(r#"{"sessions":[{"id":"s1"}"#),
            Err(AppError::Config(message))
                if message.starts_with("Hermes returned invalid session JSON:")
        ));
    }
}
