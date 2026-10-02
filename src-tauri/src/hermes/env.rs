//! Environment wrapper (ADR-013): the only place that knows about proot-distro.

use crate::config::{HermesConfig, HermesEnvironment};
use crate::termux::shell_escape;

/// Shell snippet (run in Termux) that sets `$R` to the environment's root on the Termux filesystem.
/// Supports the newer `containers/<d>/rootfs` and older `installed-rootfs/<d>` layouts.
pub fn root_expr(env: &HermesEnvironment) -> String {
    match env {
        HermesEnvironment::Termux => "R=''".into(),
        HermesEnvironment::ProotDistro { distro } => format!(
            "R=\"$PREFIX/var/lib/proot-distro/containers/{d}/rootfs\"; [ -d \"$R\" ] || R=\"$PREFIX/var/lib/proot-distro/installed-rootfs/{d}\"",
            d = distro
        ),
    }
}

/// Wraps a command so it runs inside the Hermes environment.
pub fn wrap(cfg: &HermesConfig, cmd: &str) -> String {
    let path = if cfg.path_prepend.is_empty() {
        String::new()
    } else {
        format!(
            "export PATH={}:$PATH; ",
            cfg.path_prepend
                .iter()
                .map(|p| shell_escape(p))
                .collect::<Vec<_>>()
                .join(":")
        )
    };
    let inner = format!("{path}{cmd}");
    match &cfg.environment {
        HermesEnvironment::Termux => inner,
        HermesEnvironment::ProotDistro { distro } => {
            format!(
                "proot-distro login {} -- bash -c {}",
                shell_escape(distro),
                shell_escape(&inner)
            )
        }
    }
}

/// Regex for `pgrep -f` that won't match the probing shell's own command line.
pub fn self_safe_pattern(pattern: &str) -> String {
    let mut chars = pattern.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphanumeric() => format!("[{c}]{}", chars.as_str()),
        _ => pattern.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_for_proot_with_path() {
        let cfg = HermesConfig::default();
        assert_eq!(
            wrap(&cfg, "hermes gateway run"),
            "proot-distro login debian -- bash -c 'export PATH=/root/.local/bin:$PATH; hermes gateway run'"
        );
    }

    #[test]
    fn wraps_quotes_safely() {
        let cfg = HermesConfig {
            path_prepend: vec![],
            ..Default::default()
        };
        assert_eq!(
            wrap(&cfg, "echo 'x'"),
            r"proot-distro login debian -- bash -c 'echo '\''x'\'''"
        );
    }

    #[test]
    fn termux_env_is_unwrapped() {
        let cfg = HermesConfig {
            environment: HermesEnvironment::Termux,
            path_prepend: vec![],
            ..Default::default()
        };
        assert_eq!(wrap(&cfg, "hermes gateway run"), "hermes gateway run");
        assert_eq!(root_expr(&cfg.environment), "R=''");
    }

    #[test]
    fn root_expr_covers_both_layouts() {
        let e = root_expr(&HermesEnvironment::ProotDistro {
            distro: "debian".into(),
        });
        assert!(e.contains("containers/debian/rootfs") && e.contains("installed-rootfs/debian"));
    }

    #[test]
    fn self_safe_pattern_brackets_first_char() {
        assert_eq!(
            self_safe_pattern("hermes-agent/venv/bin/python"),
            "[h]ermes-agent/venv/bin/python"
        );
        assert_eq!(self_safe_pattern("/x"), "/x");
    }
}
