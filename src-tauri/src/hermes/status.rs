//! Hermes status: one fast probe (files + /proc) parsed into a structured status. Never fabricates.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::env::{root_expr, self_safe_pattern};
use super::supervisor::DIR;
use crate::config::HermesConfig;
use crate::termux::shell_escape;

/// Heartbeat older than this while the process is alive means the gateway is stuck.
pub const STALE_SECS: i64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ComponentStatus {
    Running,
    Degraded,
    Stopped,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ProcessKind {
    Gateway,
    Cli,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HermesProcess {
    pub pid: u32,
    #[ts(type = "number | null")]
    pub uptime_secs: Option<u64>,
    pub kind: ProcessKind,
    pub cmdline: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlatformStatus {
    pub name: String,
    pub state: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SupervisorStatus {
    pub running: bool,
    pub pid: Option<u32>,
    /// Gateway restarts by the supervisor in the last 10 minutes.
    pub recent_restarts: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HermesStatus {
    pub gateway: ComponentStatus,
    pub gateway_pid: Option<u32>,
    #[ts(type = "number | null")]
    pub uptime_secs: Option<u64>,
    pub python_version: Option<String>,
    pub processes: Vec<HermesProcess>,
    /// Only reported when the state file belongs to the running gateway.
    pub platforms: Vec<PlatformStatus>,
    pub supervisor: SupervisorStatus,
    pub warnings: Vec<String>,
    /// Optional output from the user's configured status command.
    pub raw_status_output: Option<String>,
    /// "termuxSsh" or "adb" (limited).
    pub source: String,
    #[ts(type = "number")]
    pub checked_at: u64,
}

pub fn probe_script(cfg: &HermesConfig) -> String {
    let pat = shell_escape(&self_safe_pattern(&cfg.process_match));
    let home = &cfg.hermes_home;
    format!(
        "{root}; \
echo @@procs; for p in $(pgrep -f {pat}); do echo \"$p|$(ps -o etimes= -p $p 2>/dev/null | tr -d ' ')|$(tr '\\0' ' ' < /proc/$p/cmdline 2>/dev/null)\"; done; \
echo @@python; for p in $(pgrep -f {pat}); do V=$(tr '\\0' '\\n' < /proc/$p/cmdline | head -1); grep -h version_info \"$R$(dirname \"$(dirname \"$V\")\")/pyvenv.cfg\" 2>/dev/null; break; done; \
echo @@state; cat \"$R{home}/gateway_state.json\" 2>/dev/null; echo; \
echo @@sup; S=$(cat ~/{dir}/hermes-supervisor.pid 2>/dev/null); [ -n \"$S\" ] && kill -0 \"$S\" 2>/dev/null && echo \"pid=$S\"; \
echo @@suplog; tail -n 30 ~/{dir}/supervisor.log 2>/dev/null; \
echo @@now; date +%s; echo @@end",
        root = root_expr(&cfg.environment),
        dir = DIR,
    )
}

fn sections(text: &str) -> std::collections::HashMap<&str, Vec<&str>> {
    let mut out: std::collections::HashMap<&str, Vec<&str>> = Default::default();
    let mut cur = "";
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("@@") {
            cur = name.trim();
            out.entry(cur).or_default();
        } else if !cur.is_empty() {
            out.entry(cur).or_default().push(line);
        }
    }
    out
}

pub fn parse_processes(lines: &[&str], gateway_match: &str) -> Vec<HermesProcess> {
    lines
        .iter()
        .filter_map(|l| {
            let mut it = l.splitn(3, '|');
            let pid = it.next()?.trim().parse().ok()?;
            let uptime_secs = it.next().and_then(|s| s.trim().parse().ok());
            let cmdline = it.next().unwrap_or("").trim().to_string();
            let kind = if cmdline.contains(gateway_match) {
                ProcessKind::Gateway
            } else if cmdline.split_whitespace().count() == 2 && cmdline.ends_with("hermes") {
                ProcessKind::Cli
            } else {
                ProcessKind::Other
            };
            Some(HermesProcess {
                pid,
                uptime_secs,
                kind,
                cmdline,
            })
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct StateFile {
    pid: Option<u32>,
    gateway_state: Option<String>,
    exit_reason: Option<String>,
    updated_at: Option<String>,
    #[serde(default)]
    platforms: std::collections::BTreeMap<String, PlatformEntry>,
}

#[derive(Debug, Deserialize)]
struct PlatformEntry {
    state: Option<String>,
    error_message: Option<String>,
}

/// Seconds since epoch from an RFC 3339 timestamp like `2026-09-25T16:28:52.021445+00:00` (UTC only).
pub fn parse_utc_epoch(s: &str) -> Option<i64> {
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let time = time.trim_end_matches('Z');
    let (hms, offset) = match time.find(['+', '-']) {
        Some(i) => (&time[..i], &time[i..]),
        None => (time, "+00:00"),
    };
    let mut t = hms.split(':');
    let (hh, mm) = (
        t.next()?.parse::<i64>().ok()?,
        t.next()?.parse::<i64>().ok()?,
    );
    let ss = t.next()?.split('.').next()?.parse::<i64>().ok()?;
    let sign = if offset.starts_with('-') { -1 } else { 1 };
    let mut o = offset[1..].split(':');
    let off = sign
        * (o.next()?.parse::<i64>().ok()? * 3600
            + o.next().unwrap_or("0").parse::<i64>().ok()? * 60);
    // Days from civil (Howard Hinnant).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss - off)
}

pub fn parse_status(cfg: &HermesConfig, text: &str, checked_at: u64) -> HermesStatus {
    let s = sections(text);
    let get = |k: &str| s.get(k).cloned().unwrap_or_default();
    let raw_processes = get("procs");
    let processes = parse_processes(&raw_processes, &cfg.gateway_match);
    let gw = processes.iter().find(|p| p.kind == ProcessKind::Gateway);
    let now: Option<i64> = get("now").first().and_then(|l| l.trim().parse().ok());
    let python_version = get("python")
        .first()
        .and_then(|l| l.split_once('='))
        .map(|(_, v)| format!("Python {}", v.trim()));

    let mut warnings = Vec::new();
    let state: Option<StateFile> = serde_json::from_str(&get("state").join("\n")).ok();
    let mut gateway = if gw.is_some() {
        ComponentStatus::Running
    } else if !s.contains_key("procs")
        || (processes.is_empty() && raw_processes.iter().any(|line| !line.trim().is_empty()))
    {
        ComponentStatus::Unknown
    } else {
        ComponentStatus::Stopped
    };
    let mut platforms = Vec::new();

    if let Some(st) = &state {
        let owned_by_running = gw.is_some_and(|g| st.pid == Some(g.pid));
        if owned_by_running {
            let age = st
                .updated_at
                .as_deref()
                .and_then(parse_utc_epoch)
                .zip(now)
                .map(|(u, n)| n - u);
            if st.gateway_state.as_deref() == Some("degraded") {
                gateway = ComponentStatus::Degraded;
                warnings.push(format!(
                    "Gateway reports degraded: {}",
                    st.exit_reason.clone().unwrap_or_default()
                ));
            } else if age.is_some_and(|a| a > STALE_SECS) {
                gateway = ComponentStatus::Degraded;
                warnings.push(format!(
                    "Gateway heartbeat is stale ({} s old) — the event loop may be stuck.",
                    age.unwrap_or(0)
                ));
            }
            platforms = st
                .platforms
                .iter()
                .map(|(name, p)| PlatformStatus {
                    name: name.clone(),
                    state: p.state.clone().unwrap_or_else(|| "unknown".into()),
                    error: p.error_message.clone(),
                })
                .collect();
        } else if gw.is_none() && st.gateway_state.as_deref() == Some("running") {
            warnings.push(format!(
                "State file says running (pid {}) but no gateway process exists — it exited without a supervisor.",
                st.pid.map(|p| p.to_string()).unwrap_or_else(|| "?".into())
            ));
        }
    }

    let sup_pid = get("sup")
        .iter()
        .find_map(|l| l.strip_prefix("pid="))
        .and_then(|p| p.trim().parse().ok());
    let recent_restarts = now
        .map(|n| {
            get("suplog")
                .iter()
                .filter(|l| l.contains("restarting in"))
                .filter_map(|l| {
                    l.split_whitespace()
                        .next()
                        .and_then(|e| e.parse::<i64>().ok())
                })
                .filter(|t| n - t <= 600)
                .count() as u32
        })
        .unwrap_or(0);
    if recent_restarts >= 5 {
        warnings.push(format!(
            "Gateway crash loop: {recent_restarts} restarts in the last 10 minutes."
        ));
    }
    if gw.is_some() && sup_pid.is_none() {
        warnings.push(
            "Gateway was started outside the app; it won't be restarted automatically if it exits."
                .into(),
        );
    }
    if processes.iter().any(|p| p.kind == ProcessKind::Cli) {
        warnings.push("An interactive Hermes CLI session is also running in Termux.".into());
    }

    HermesStatus {
        gateway,
        gateway_pid: gw.map(|g| g.pid),
        uptime_secs: gw.and_then(|g| g.uptime_secs),
        python_version,
        processes,
        platforms,
        supervisor: SupervisorStatus {
            running: sup_pid.is_some(),
            pid: sup_pid,
            recent_restarts,
        },
        warnings,
        raw_status_output: None,
        source: "termuxSsh".into(),
        checked_at,
    }
}

/// Limited status from `adb shell` when the Termux bridge is down (process list only).
pub fn parse_adb_processes(cfg: &HermesConfig, text: &str, checked_at: u64) -> HermesStatus {
    let lines: Vec<&str> = text.lines().collect();
    let processes = parse_processes(&lines, &cfg.gateway_match);
    let gw = processes.iter().find(|p| p.kind == ProcessKind::Gateway);
    HermesStatus {
        gateway: if gw.is_some() {
            ComponentStatus::Running
        } else {
            ComponentStatus::Stopped
        },
        gateway_pid: gw.map(|g| g.pid),
        uptime_secs: gw.and_then(|g| g.uptime_secs),
        python_version: None,
        processes,
        platforms: vec![],
        supervisor: SupervisorStatus {
            running: false,
            pid: None,
            recent_restarts: 0,
        },
        warnings: vec!["Termux bridge not connected — showing process info from ADB only.".into()],
        raw_status_output: None,
        source: "adb".into(),
        checked_at,
    }
}

pub fn adb_probe_script(cfg: &HermesConfig) -> String {
    format!(
        "for p in $(pgrep -f {pat}); do echo \"$p|$(ps -o ETIME= -p $p 2>/dev/null | tr -d ' ' | awk -F'[-:]' '{{s=0; for(i=1;i<=NF;i++) s=s*60+$i; if(NF==4) s=$1*86400+$2*3600+$3*60+$4; print s}}')|$(tr '\\0' ' ' < /proc/$p/cmdline 2>/dev/null)\"; done",
        pat = shell_escape(&self_safe_pattern(&cfg.process_match))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const GW: &str = "15351|3600|/usr/local/lib/hermes-agent/venv/bin/python /root/.local/bin/hermes gateway run ";
    const CLI: &str =
        "15400|27|/usr/local/lib/hermes-agent/venv/bin/python /root/.local/bin/hermes ";

    fn probe(procs: &[&str], state: &str, sup: &str, now: i64) -> String {
        format!(
            "@@procs\n{}\n@@python\nversion_info = 3.13.5\n@@state\n{state}\n@@sup\n{sup}\n@@suplog\n@@now\n{now}\n@@end\n",
            procs.join("\n")
        )
    }

    #[test]
    fn running_gateway_with_fresh_heartbeat_and_platforms() {
        let state = r#"{"pid":15351,"gateway_state":"running","updated_at":"2026-10-02T09:00:00+00:00","platforms":{"telegram":{"state":"connected","error_message":null}}}"#;
        let now = parse_utc_epoch("2026-10-02T09:01:00+00:00").unwrap();
        let s = parse_status(
            &HermesConfig::default(),
            &probe(&[GW], state, "pid=999", now),
            0,
        );
        assert_eq!(s.gateway, ComponentStatus::Running);
        assert_eq!(s.gateway_pid, Some(15351));
        assert_eq!(s.uptime_secs, Some(3600));
        assert_eq!(s.python_version.as_deref(), Some("Python 3.13.5"));
        assert_eq!(s.platforms[0].name, "telegram");
        assert!(s.supervisor.running);
        assert!(s.warnings.is_empty(), "{:?}", s.warnings);
    }

    #[test]
    fn stale_state_file_of_dead_gateway_is_not_trusted() {
        // Real case: state said running (pid 14920) days after the watchdog killed it.
        let state = r#"{"pid":14920,"gateway_state":"running","updated_at":"2026-09-25T16:28:52.021445+00:00","platforms":{"telegram":{"state":"retrying","error_message":"telegram connect timed out after 30s"}}}"#;
        let s = parse_status(
            &HermesConfig::default(),
            &probe(&[CLI], state, "", 1_790_000_000),
            0,
        );
        assert_eq!(s.gateway, ComponentStatus::Stopped);
        assert!(s.platforms.is_empty());
        assert!(s
            .warnings
            .iter()
            .any(|w| w.contains("no gateway process exists")));
        assert!(s
            .warnings
            .iter()
            .any(|w| w.contains("interactive Hermes CLI")));
        assert_eq!(s.processes[0].kind, ProcessKind::Cli);
    }

    #[test]
    fn stuck_heartbeat_is_degraded() {
        let state =
            r#"{"pid":15351,"gateway_state":"running","updated_at":"2026-10-02T09:00:00+00:00"}"#;
        let now = parse_utc_epoch("2026-10-02T09:05:00+00:00").unwrap();
        let s = parse_status(&HermesConfig::default(), &probe(&[GW], state, "", now), 0);
        assert_eq!(s.gateway, ComponentStatus::Degraded);
        assert!(s.warnings.iter().any(|w| w.contains("heartbeat is stale")));
        assert!(s.warnings.iter().any(|w| w.contains("outside the app")));
    }

    #[test]
    fn empty_process_probe_means_stopped_but_missing_or_malformed_probe_is_unknown() {
        let s = parse_status(&HermesConfig::default(), &probe(&[], "", "", 0), 0);
        assert_eq!(s.gateway, ComponentStatus::Stopped);
        assert!(s.processes.is_empty());
        let missing = parse_status(&HermesConfig::default(), "garbage", 0);
        assert_eq!(missing.gateway, ComponentStatus::Unknown);
        let malformed = parse_status(&HermesConfig::default(), "@@procs\nnot|a|pid", 0);
        assert_eq!(malformed.gateway, ComponentStatus::Unknown);
    }

    #[test]
    fn crash_loop_detected_from_supervisor_log() {
        let lines: Vec<String> = (0..5)
            .map(|i| format!("{} restarting in 2s", 1000 + i))
            .collect();
        let text = format!("@@procs\n@@suplog\n{}\n@@now\n1100\n", lines.join("\n"));
        let s = parse_status(&HermesConfig::default(), &text, 0);
        assert_eq!(s.supervisor.recent_restarts, 5);
        assert!(s.warnings.iter().any(|w| w.contains("crash loop")));
    }

    #[test]
    fn parses_rfc3339() {
        assert_eq!(parse_utc_epoch("1970-01-01T00:00:00+00:00"), Some(0));
        assert_eq!(
            parse_utc_epoch("2026-09-25T16:28:52.021445+00:00"),
            Some(1_790_353_732)
        );
        assert_eq!(
            parse_utc_epoch("2000-01-01T02:00:00+02:00"),
            Some(946_684_800)
        );
        assert_eq!(parse_utc_epoch("nope"), None);
    }

    #[test]
    fn probe_script_uses_root_and_safe_pattern() {
        let s = probe_script(&HermesConfig::default());
        assert!(s.contains("containers/debian/rootfs"));
        assert!(s.contains("[h]ermes-agent/venv/bin/python"));
        assert!(s.contains("$R/root/.hermes/gateway_state.json"));
    }

    #[test]
    fn adb_fallback_is_marked_limited() {
        let s = parse_adb_processes(&HermesConfig::default(), GW, 0);
        assert_eq!(s.gateway, ComponentStatus::Running);
        assert_eq!(s.source, "adb");
        assert!(!s.warnings.is_empty());
    }
}
