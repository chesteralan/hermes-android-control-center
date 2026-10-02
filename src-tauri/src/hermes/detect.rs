//! Finds Hermes installs in Termux and every proot-distro, without logging into proot (fast).

use serde::Serialize;
use ts_rs::TS;

use crate::config::HermesEnvironment;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HermesCandidate {
    pub environment: HermesEnvironment,
    /// Path inside the environment.
    pub binary_path: String,
    /// Directory to prepend to PATH inside the environment.
    pub bin_dir: String,
    pub version: Option<String>,
    pub python_version: Option<String>,
    pub hermes_home_exists: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HermesInstallReport {
    pub candidates: Vec<HermesCandidate>,
    pub searched: Vec<String>,
}

/// Prints `env|binpath|python|home` lines. Symlinks are resolved inside the rootfs.
pub const SCRIPT: &str = r#"B="/root/.local/bin /usr/local/bin /usr/bin"
check() { env="$1"; R="$2"; home="$3"
  for d in $B; do
    f="$R$d/hermes"; [ -e "$f" ] || [ -L "$f" ] || continue
    t=$(readlink "$f"); case "$t" in /*) t="$R$t";; "") t="$f";; *) t="$(dirname "$f")/$t";; esac
    py=$(grep -h version_info "$(dirname "$(dirname "$t")")/pyvenv.cfg" 2>/dev/null | cut -d= -f2 | tr -d ' ')
    [ -d "$R$home" ] && h=yes || h=no
    echo "$env|$d/hermes|$py|$h"; return
  done
  echo "$env|-"
}
T=$(command -v hermes 2>/dev/null); if [ -n "$T" ]; then echo "termux|$T||$([ -d ~/.hermes ] && echo yes || echo no)"; else echo "termux|-"; fi
for R in "$PREFIX"/var/lib/proot-distro/containers/*/rootfs "$PREFIX"/var/lib/proot-distro/installed-rootfs/*; do
  [ -d "$R" ] || continue
  case "$R" in */rootfs) n=$(basename "$(dirname "$R")");; *) n=$(basename "$R");; esac
  check "proot:$n" "$R" /root/.hermes
done"#;

pub fn parse(out: &str) -> HermesInstallReport {
    let mut report = HermesInstallReport {
        candidates: vec![],
        searched: vec![],
    };
    for line in out.lines() {
        let f: Vec<&str> = line.trim().split('|').collect();
        let Some(env_name) = f.first().filter(|s| !s.is_empty()) else {
            continue;
        };
        if !report.searched.iter().any(|s| s == env_name) {
            report.searched.push(env_name.to_string());
        }
        let Some(bin) = f.get(1).filter(|b| **b != "-" && !b.is_empty()) else {
            continue;
        };
        let environment = match env_name.strip_prefix("proot:") {
            Some(d) => HermesEnvironment::ProotDistro {
                distro: d.to_string(),
            },
            None => HermesEnvironment::Termux,
        };
        let bin_dir = bin
            .rsplit_once('/')
            .map(|(d, _)| d.to_string())
            .unwrap_or_default();
        report.candidates.push(HermesCandidate {
            environment,
            binary_path: bin.to_string(),
            bin_dir,
            version: f.get(4).filter(|s| !s.is_empty()).map(|v| v.to_string()),
            python_version: f
                .get(2)
                .filter(|s| !s.is_empty())
                .map(|v| format!("Python {v}")),
            hermes_home_exists: f.get(3) == Some(&"yes"),
        });
    }
    report
}

pub fn first_version_line(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proot_and_termux_lines() {
        let r = parse(
            "termux|-\nproot:debian|/root/.local/bin/hermes|3.13.5|yes|0.21.5\nproot:ubuntu|-\n",
        );
        assert_eq!(r.searched, vec!["termux", "proot:debian", "proot:ubuntu"]);
        assert_eq!(r.candidates.len(), 1);
        let c = &r.candidates[0];
        assert_eq!(
            c.environment,
            HermesEnvironment::ProotDistro {
                distro: "debian".into()
            }
        );
        assert_eq!(c.bin_dir, "/root/.local/bin");
        assert_eq!(c.version.as_deref(), Some("0.21.5"));
        assert_eq!(c.python_version.as_deref(), Some("Python 3.13.5"));
        assert!(c.hermes_home_exists);
    }

    #[test]
    fn none_found_and_garbage() {
        assert!(parse("termux|-\n").candidates.is_empty());
        assert!(parse("||||\n\n").candidates.is_empty());
    }

    #[test]
    fn extracts_a_version_line_without_fabricating_one() {
        assert_eq!(
            first_version_line("\nHermes Agent 0.21.5\n"),
            Some("Hermes Agent 0.21.5".into())
        );
        assert_eq!(first_version_line("  \n"), None);
    }
}
