//! Termux-side gateway supervisor (ADR-015): relaunches `hermes gateway run` unless stopped on purpose.

use super::env::{self_safe_pattern, wrap};
use crate::config::HermesConfig;
use crate::termux::shell_escape;

/// Directory under the Termux home for app-managed files.
pub const DIR: &str = ".hacc";
const VERSION: &str = "hacc-supervisor v1";

pub fn script(cfg: &HermesConfig) -> String {
    format!(
        r#"#!/data/data/com.termux/files/usr/bin/sh
# {VERSION} — managed by Hermes Control Center; changes are overwritten.
D="$HOME/{DIR}"
mkdir -p "$D"
echo $$ > "$D/hermes-supervisor.pid"
rm -f "$D/hermes.stop"
log() {{ echo "$(date +%s) $(date '+%Y-%m-%d %H:%M:%S') $*" >> "$D/supervisor.log"; }}
n=0
while :; do
  started=$(date +%s)
  log "starting gateway"
  {gateway} >> "$D/gateway-console.log" 2>&1
  code=$?
  log "gateway exited code=$code"
  if [ -f "$D/hermes.stop" ]; then rm -f "$D/hermes.stop"; log "stopped on request"; break; fi
  [ $(( $(date +%s) - started )) -ge 600 ] && n=0
  case $n in 0) d=2;; 1) d=5;; 2) d=15;; *) d=30;; esac
  n=$((n+1))
  log "restarting in ${{d}}s"
  sleep "$d"
done
rm -f "$D/hermes-supervisor.pid"
"#,
        gateway = wrap(cfg, &format!("exec {}", cfg.gateway_command)),
    )
}

fn sup_pid_check() -> String {
    // Grouped so `! {sup}` negates the whole liveness test instead of just the
    // assignment; a bare `A; B` list is decided by B alone.
    format!(
        "{{ S=$(cat ~/{DIR}/hermes-supervisor.pid 2>/dev/null); [ -n \"$S\" ] && kill -0 \"$S\" 2>/dev/null; }}"
    )
}

fn gateway_pids(cfg: &HermesConfig) -> String {
    let pat = format!(
        "{}.*{}",
        self_safe_pattern(&cfg.process_match),
        cfg.gateway_match
    );
    format!("pgrep -f {}", shell_escape(&pat))
}

/// Writes the supervisor script (only if it changed) and starts it detached unless already running.
pub fn start_command(cfg: &HermesConfig) -> String {
    let body = script(cfg);
    format!(
        "mkdir -p ~/{DIR} && \
if ! cmp -s ~/{DIR}/hermes-supervisor.sh - <<'HACC_EOF'\n{body}HACC_EOF\nthen cat > ~/{DIR}/hermes-supervisor.sh <<'HACC_EOF'\n{body}HACC_EOF\nfi; \
chmod 700 ~/{DIR}/hermes-supervisor.sh; \
if {sup}; then echo 'supervisor already running'; \
elif [ -n \"$({gw})\" ]; then echo 'gateway already running (started outside the app)'; exit 3; \
else nohup setsid sh ~/{DIR}/hermes-supervisor.sh > /dev/null 2>&1 < /dev/null & echo 'supervisor started'; fi",
        sup = sup_pid_check(),
        gw = gateway_pids(cfg),
    )
}

/// Detached (unsupervised) start.
pub fn start_detached_command(cfg: &HermesConfig) -> String {
    format!(
        "mkdir -p ~/{DIR} && nohup setsid {} >> ~/{DIR}/gateway-console.log 2>&1 < /dev/null & echo started",
        wrap(cfg, &format!("exec {}", cfg.gateway_command))
    )
}

/// Restarts a gateway that was started without the app supervisor.
pub fn restart_detached_command(cfg: &HermesConfig) -> String {
    let gw = gateway_pids(cfg);
    format!(
        "P=$({gw}); if [ -n \"$P\" ]; then kill -TERM $P; n=0; while [ $n -lt 10 ] && [ -n \"$({gw})\" ]; do sleep 1; n=$((n+1)); done; fi; {start}",
        start = start_detached_command(cfg),
    )
}

/// Stop: set the stop flag so the supervisor exits, then SIGTERM the gateway.
pub fn stop_command(cfg: &HermesConfig) -> String {
    format!(
        "mkdir -p ~/{DIR}; if {sup}; then touch ~/{DIR}/hermes.stop; fi; P=$({gw}); \
if [ -z \"$P\" ]; then echo 'gateway not running'; exit 4; fi; kill -TERM $P && echo \"sent TERM to $P\"",
        sup = sup_pid_check(),
        gw = gateway_pids(cfg),
    )
}

/// Graceful: SIGUSR1 (Hermes drains in-flight turns, then exits). Otherwise SIGTERM.
/// Either way the supervisor relaunches it.
pub fn restart_command(cfg: &HermesConfig, graceful: bool) -> String {
    let sig = if graceful { "USR1" } else { "TERM" };
    format!(
        "P=$({gw}); if [ -z \"$P\" ]; then echo 'gateway not running'; exit 4; fi; \
if ! {sup}; then echo 'not supervised: start Hermes from the app to enable restarts'; exit 5; fi; \
kill -{sig} $P && echo \"sent {sig} to $P\"",
        sup = sup_pid_check(),
        gw = gateway_pids(cfg),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_runs_wrapped_gateway_and_honours_stop_flag() {
        let s = script(&HermesConfig::default());
        assert!(s.contains("proot-distro login debian -- bash -c 'export PATH=/root/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin; unset CC CXX CPP CFLAGS CXXFLAGS CPPFLAGS LDFLAGS _PYTHON_HOST_PLATFORM _PYTHON_SYSCONFIGDATA_NAME UV_PYTHON UV_PYTHON_INSTALL_DIR PYTHONHOME PYTHONPATH; exec hermes gateway run'"));
        assert!(s.contains("hermes.stop"));
        assert!(s.contains(VERSION));
        assert!(s.contains("restarting in ${d}s"));
    }

    #[test]
    fn start_is_idempotent_and_detached() {
        let c = start_command(&HermesConfig::default());
        assert!(c.contains("cmp -s"));
        assert!(c.contains("nohup setsid sh ~/.hacc/hermes-supervisor.sh"));
        assert!(c.contains("supervisor already running"));
        assert!(c.contains("started outside the app"));
    }

    /// The restart guard must treat a live supervisor PID as supervised. The
    /// generated script used to negate a `;` list (`! A; B`), so the condition
    /// was decided by the *last* command and the "not supervised" branch ran
    /// even when the PID was alive (live QA 2026-10-29, exit code 5).
    #[cfg(unix)]
    #[test]
    fn restart_reaches_a_live_supervisor() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!("hacc-restart-sup-{}", std::process::id()));
        let home = root.join("home");
        let bin = root.join("bin");
        std::fs::create_dir_all(home.join(".hacc")).unwrap();
        std::fs::create_dir_all(&bin).unwrap();

        let mut supervisor = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let mut gateway = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        std::fs::write(
            home.join(".hacc/hermes-supervisor.pid"),
            supervisor.id().to_string(),
        )
        .unwrap();

        // `pgrep` is stubbed: the script only needs one live gateway PID.
        let pgrep = bin.join("pgrep");
        let mut stub = std::fs::File::create(&pgrep).unwrap();
        writeln!(stub, "#!/bin/sh\necho {}", gateway.id()).unwrap();
        drop(stub);
        std::fs::set_permissions(&pgrep, std::fs::Permissions::from_mode(0o755)).unwrap();

        let output = std::process::Command::new("bash")
            .arg("-c")
            .arg(restart_command(&HermesConfig::default(), false))
            .env("HOME", &home)
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    bin.display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .output()
            .unwrap();

        let _ = supervisor.kill();
        let _ = gateway.kill();
        let _ = supervisor.wait();
        let _ = gateway.wait();
        let _ = std::fs::remove_dir_all(&root);

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.contains("not supervised"),
            "live supervisor must not hit the guard: {stdout}"
        );
        assert!(
            stdout.contains("sent TERM"),
            "restart must signal the gateway: {stdout}"
        );
        assert_eq!(output.status.code(), Some(0), "stdout={stdout}");
    }

    #[test]
    fn stop_and_restart_signal_only_the_gateway() {
        let cfg = HermesConfig::default();
        assert!(stop_command(&cfg).contains("touch ~/.hacc/hermes.stop"));
        assert!(
            stop_command(&cfg).contains("pgrep -f '[h]ermes-agent/venv/bin/python.*gateway run'")
        );
        assert!(restart_command(&cfg, true).contains("kill -USR1"));
        assert!(restart_command(&cfg, false).contains("kill -TERM"));
    }
}
