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
    match &cfg.environment {
        HermesEnvironment::Termux => {
            if cfg.path_prepend.is_empty() {
                cmd.to_string()
            } else {
                format!(
                    "export PATH={}:$PATH; {}",
                    cfg.path_prepend
                        .iter()
                        .map(|p| shell_escape(p))
                        .collect::<Vec<_>>()
                        .join(":"),
                    cmd
                )
            }
        }
        HermesEnvironment::ProotDistro { distro } => {
            // Inside proot the environment must be Debian's: Termux exports
            // CC/CFLAGS/android python env into every ssh session and
            // proot-distro passes them through, which made builds target
            // aarch64-linux-android (M11 install failures). Replace PATH
            // outright (prepend first) and drop the toolchain variables.
            let mut path = cfg.path_prepend.clone();
            path.extend(
                [
                    "/usr/local/sbin",
                    "/usr/local/bin",
                    "/usr/sbin",
                    "/usr/bin",
                    "/sbin",
                    "/bin",
                ]
                .iter()
                .map(|entry| (*entry).to_string()),
            );
            let inner = format!(
                "export PATH={}; unset CC CXX CPP CFLAGS CXXFLAGS CPPFLAGS LDFLAGS \
                 _PYTHON_HOST_PLATFORM _PYTHON_SYSCONFIGDATA_NAME UV_PYTHON \
                 UV_PYTHON_INSTALL_DIR PYTHONHOME PYTHONPATH; {}",
                path.iter()
                    .map(|p| shell_escape(p))
                    .collect::<Vec<_>>()
                    .join(":"),
                cmd
            );
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
            "proot-distro login debian -- bash -c 'export PATH=/root/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin; unset CC CXX CPP CFLAGS CXXFLAGS CPPFLAGS LDFLAGS _PYTHON_HOST_PLATFORM _PYTHON_SYSCONFIGDATA_NAME UV_PYTHON UV_PYTHON_INSTALL_DIR PYTHONHOME PYTHONPATH; hermes gateway run'"
        );
    }

    #[test]
    fn wraps_proot_without_path_prepend_still_sanitizes() {
        let cfg = HermesConfig {
            path_prepend: vec![],
            ..Default::default()
        };
        assert_eq!(
            wrap(&cfg, "echo 'x'"),
            r"proot-distro login debian -- bash -c 'export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin; unset CC CXX CPP CFLAGS CXXFLAGS CPPFLAGS LDFLAGS _PYTHON_HOST_PLATFORM _PYTHON_SYSCONFIGDATA_NAME UV_PYTHON UV_PYTHON_INSTALL_DIR PYTHONHOME PYTHONPATH; echo '\''x'\'''"
        );
    }

    #[test]
    fn wraps_proot_strips_inherited_android_toolchain() {
        // Termux exports CC/CFLAGS/python env into every ssh session;
        // proot-distro login passes them through, which made builds inside
        // Debian target aarch64-linux-android (M11 install failures).
        let wrapped = wrap(&HermesConfig::default(), "hermes --version");
        assert!(
            !wrapped.contains("$PATH"),
            "must not inherit Termux PATH entries"
        );
        assert!(
            wrapped.contains("unset CC "),
            "must drop the android CC wrapper"
        );
        assert!(wrapped.contains("_PYTHON_HOST_PLATFORM"));
        assert!(wrapped.contains("PYTHONHOME"));
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
