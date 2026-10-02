//! Termux readiness checklist (M4-T4). Each item explains how to fix itself.

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CheckStatus {
    Ok,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheckItem {
    pub id: String,
    pub label: String,
    pub status: CheckStatus,
    pub detail: Option<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TermuxCheck {
    pub ready: bool,
    pub items: Vec<CheckItem>,
    /// Installed proot-distro distributions (both `installed-rootfs` and newer `containers` layouts).
    pub distros: Vec<String>,
}

pub struct Checklist {
    items: Vec<CheckItem>,
    failed: bool,
}

impl Checklist {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            failed: false,
        }
    }

    pub fn ok(&mut self, id: &str, label: &str, detail: Option<String>) {
        self.items.push(CheckItem {
            id: id.into(),
            label: label.into(),
            status: CheckStatus::Ok,
            detail,
            hint: None,
        });
    }

    pub fn fail(&mut self, id: &str, label: &str, detail: Option<String>, hint: &str) {
        self.failed = true;
        self.items.push(CheckItem {
            id: id.into(),
            label: label.into(),
            status: CheckStatus::Failed,
            detail,
            hint: Some(hint.into()),
        });
    }

    pub fn skip(&mut self, id: &str, label: &str) {
        self.items.push(CheckItem {
            id: id.into(),
            label: label.into(),
            status: CheckStatus::Skipped,
            detail: None,
            hint: None,
        });
    }

    pub fn has_failed(&self) -> bool {
        self.failed
    }

    pub fn finish(self, distros: Vec<String>) -> TermuxCheck {
        TermuxCheck {
            ready: !self.failed,
            items: self.items,
            distros,
        }
    }
}

impl Default for Checklist {
    fn default() -> Self {
        Self::new()
    }
}

/// Command run over SSH to probe the Termux environment.
pub const PROBE: &str = "echo \"prefix=$PREFIX\"; \
command -v proot-distro >/dev/null 2>&1 && echo proot-distro=yes || echo proot-distro=no; \
for d in \"$PREFIX/var/lib/proot-distro/installed-rootfs\"/* \"$PREFIX/var/lib/proot-distro/containers\"/*; do \
[ -d \"$d\" ] && echo \"distro=${d##*/}\"; done; true";

#[derive(Debug, Default, PartialEq)]
pub struct Probe {
    pub prefix: Option<String>,
    pub proot_distro: bool,
    pub distros: Vec<String>,
}

pub fn parse_probe(out: &str) -> Probe {
    let mut p = Probe::default();
    for line in out.lines() {
        if let Some(v) = line.strip_prefix("prefix=") {
            p.prefix = Some(v.trim().to_string()).filter(|s| !s.is_empty());
        } else if line.trim() == "proot-distro=yes" {
            p.proot_distro = true;
        } else if let Some(d) = line.strip_prefix("distro=") {
            let d = d.trim().to_string();
            if !d.is_empty() && !p.distros.contains(&d) {
                p.distros.push(d);
            }
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_probe_both_layouts() {
        let p = parse_probe(
            "prefix=/data/data/com.termux/files/usr\nproot-distro=yes\ndistro=debian\ndistro=debian\ndistro=ubuntu\n",
        );
        assert_eq!(p.prefix.as_deref(), Some("/data/data/com.termux/files/usr"));
        assert!(p.proot_distro);
        assert_eq!(p.distros, vec!["debian", "ubuntu"]);
        assert_eq!(parse_probe("prefix=\nproot-distro=no\n"), Probe::default());
    }

    #[test]
    fn checklist_ready_only_without_failures() {
        let mut c = Checklist::new();
        c.ok("a", "A", None);
        c.skip("b", "B");
        assert!(c.finish(vec![]).ready);
        let mut c = Checklist::new();
        c.fail("a", "A", None, "fix it");
        let r = c.finish(vec![]);
        assert!(!r.ready);
        assert_eq!(r.items[0].hint.as_deref(), Some("fix it"));
    }
}
