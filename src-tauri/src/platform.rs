//! All OS-specific behavior lives here (ADR-016).

use std::path::{Path, PathBuf};

use tokio::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Windows,
    Linux,
}

pub fn current_os() -> Os {
    if cfg!(target_os = "macos") {
        Os::MacOs
    } else if cfg!(target_os = "windows") {
        Os::Windows
    } else {
        Os::Linux
    }
}

pub fn adb_binary_name(os: Os) -> &'static str {
    match os {
        Os::Windows => "adb.exe",
        _ => "adb",
    }
}

/// Environment lookups injected for testability.
pub trait Env: Sync {
    fn var(&self, key: &str) -> Option<String>;
    fn home(&self) -> Option<PathBuf>;
}

pub struct RealEnv;

impl Env for RealEnv {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|v| !v.is_empty())
    }

    fn home(&self) -> Option<PathBuf> {
        self.var("HOME")
            .or_else(|| self.var("USERPROFILE"))
            .map(PathBuf::from)
    }
}

/// Well-known adb install locations, in priority order (after configured path and PATH).
pub fn adb_candidates(os: Os, env: &dyn Env) -> Vec<PathBuf> {
    let bin = adb_binary_name(os);
    let mut out: Vec<PathBuf> = Vec::new();
    let sdk = |root: String| PathBuf::from(root).join("platform-tools").join(bin);
    match os {
        Os::MacOs => {
            out.push("/opt/homebrew/bin/adb".into());
            out.push("/usr/local/bin/adb".into());
        }
        Os::Linux => {
            out.push("/usr/bin/adb".into());
            out.push("/usr/local/bin/adb".into());
            out.push("/opt/android-sdk/platform-tools/adb".into());
            out.push("/snap/bin/adb".into());
        }
        Os::Windows => {}
    }
    for key in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(root) = env.var(key) {
            out.push(sdk(root));
        }
    }
    match os {
        Os::MacOs => {
            if let Some(h) = env.home() {
                out.push(h.join("Library/Android/sdk/platform-tools/adb"));
            }
        }
        Os::Linux => {
            if let Some(h) = env.home() {
                out.push(h.join("Android/Sdk/platform-tools/adb"));
            }
        }
        Os::Windows => {
            if let Some(local) = env.var("LOCALAPPDATA") {
                let local = PathBuf::from(local);
                out.push(local.join(r"Android\Sdk\platform-tools\adb.exe"));
                out.push(local.join(r"Microsoft\WinGet\Links\adb.exe"));
            }
            if let Some(h) = env.home() {
                out.push(h.join(r"scoop\shims\adb.exe"));
                out.push(h.join(r"scoop\apps\adb\current\platform-tools\adb.exe"));
            }
            out.push(r"C:\ProgramData\chocolatey\bin\adb.exe".into());
        }
    }
    out.dedup();
    out
}

/// Look up `bin` on the PATH (GUI apps on macOS often get a minimal PATH).
pub fn find_on_path(bin: &str, env: &dyn Env) -> Option<PathBuf> {
    let path = env.var("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(bin))
        .find(|p| is_file(p))
}

pub fn is_file(p: &Path) -> bool {
    p.is_file()
}

/// Restrict a secret file (e.g. SSH private key) to the current user.
pub fn restrict_file(p: &Path) -> crate::error::AppResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600))?;
    }
    // Windows: the app data dir under %APPDATA% is already private to the user (M12-T3 adds explicit ACLs).
    #[cfg(not(unix))]
    let _ = p;
    Ok(())
}

/// Per-OS process flags (hide console windows on Windows).
pub fn configure_command(cmd: &mut Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct FakeEnv(HashMap<&'static str, &'static str>);

    impl Env for FakeEnv {
        fn var(&self, key: &str) -> Option<String> {
            self.0.get(key).map(|s| s.to_string())
        }
        fn home(&self) -> Option<PathBuf> {
            self.var("HOME").map(PathBuf::from)
        }
    }

    #[test]
    fn macos_candidates_in_priority_order() {
        let env = FakeEnv(HashMap::from([
            ("HOME", "/Users/u"),
            ("ANDROID_HOME", "/sdk"),
        ]));
        let c = adb_candidates(Os::MacOs, &env);
        assert_eq!(c[0], PathBuf::from("/opt/homebrew/bin/adb"));
        assert_eq!(c[1], PathBuf::from("/usr/local/bin/adb"));
        assert!(c.contains(&PathBuf::from("/sdk/platform-tools/adb")));
        assert_eq!(
            c.last().unwrap(),
            &PathBuf::from("/Users/u/Library/Android/sdk/platform-tools/adb")
        );
    }

    #[test]
    fn windows_candidates_use_exe() {
        let env = FakeEnv(HashMap::from([(
            "LOCALAPPDATA",
            r"C:\Users\u\AppData\Local",
        )]));
        let c = adb_candidates(Os::Windows, &env);
        assert!(c.iter().all(|p| p.to_string_lossy().ends_with("adb.exe")));
    }

    #[test]
    fn linux_candidates_include_sdk_home() {
        let env = FakeEnv(HashMap::from([("HOME", "/home/u")]));
        let c = adb_candidates(Os::Linux, &env);
        assert_eq!(c[0], PathBuf::from("/usr/bin/adb"));
        assert!(c.contains(&PathBuf::from("/home/u/Android/Sdk/platform-tools/adb")));
    }
}
