//! All OS-specific behavior lives here (ADR-016).

use std::path::{Path, PathBuf};

use process_wrap::tokio::CommandWrap;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use linux::LinuxPlatform as NativePlatform;
#[cfg(target_os = "macos")]
use macos::MacOsPlatform as NativePlatform;
#[cfg(target_os = "windows")]
use windows::WindowsPlatform as NativePlatform;

trait Platform {
    fn restrict_file(path: &Path) -> crate::error::AppResult<()>;
    fn configure_command(command: &mut CommandWrap);
}

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

pub fn supports_in_app_updates(os: Os, is_appimage: bool) -> bool {
    os != Os::Linux || is_appimage
}

pub fn adb_install_hint(os: Os) -> &'static str {
    match os {
        Os::MacOs => "Install with brew install android-platform-tools, or choose the Android SDK platform-tools binary in Settings.",
        Os::Windows => "Install with winget install Google.PlatformTools or scoop install adb, or choose the Android SDK adb.exe in Settings.",
        Os::Linux => "Install adb with apt install adb, dnf install android-tools, or pacman -S android-tools, or choose the Android SDK binary in Settings.",
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
            out.push("/var/lib/flatpak/exports/bin/adb".into());
        }
        Os::Windows => {
            if let Some(local) = env.var("LOCALAPPDATA") {
                out.push(PathBuf::from(local).join(r"Android\Sdk\platform-tools\adb.exe"));
            }
        }
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
                out.push(h.join(".local/share/flatpak/exports/bin/adb"));
            }
        }
        Os::Windows => {
            if let Some(local) = env.var("LOCALAPPDATA") {
                let local = PathBuf::from(local);
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
    NativePlatform::restrict_file(p)
}

/// Per-OS process flags (hide console windows on Windows).
pub fn configure_command(cmd: &mut CommandWrap) {
    NativePlatform::configure_command(cmd);
}

pub async fn prepare_process(program: &Path, args: &[String]) -> crate::error::AppResult<()> {
    #[cfg(target_os = "windows")]
    windows::prepare_adb_server(program, args).await?;
    #[cfg(not(target_os = "windows"))]
    let _ = (program, args);
    Ok(())
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
    fn package_installs_use_package_manager_updates() {
        assert!(!supports_in_app_updates(Os::Linux, false));
        assert!(supports_in_app_updates(Os::Linux, true));
        assert!(supports_in_app_updates(Os::Windows, false));
        assert!(supports_in_app_updates(Os::MacOs, false));
        assert!(adb_install_hint(Os::Windows).contains("adb.exe"));
        assert!(adb_install_hint(Os::Linux).contains("android-tools"));
        assert!(adb_install_hint(Os::MacOs).contains("brew"));
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
        let env = FakeEnv(HashMap::from([
            ("LOCALAPPDATA", r"C:\Users\u\AppData\Local"),
            ("HOME", r"C:\Users\u"),
            ("ANDROID_HOME", r"C:\sdk"),
        ]));
        let c = adb_candidates(Os::Windows, &env);
        assert!(c.iter().all(|p| p.to_string_lossy().ends_with("adb.exe")));
        assert_eq!(
            c[0],
            PathBuf::from(r"C:\Users\u\AppData\Local").join(r"Android\Sdk\platform-tools\adb.exe")
        );
        assert!(c.contains(
            &PathBuf::from(r"C:\sdk")
                .join("platform-tools")
                .join("adb.exe")
        ));
        assert!(c.contains(&PathBuf::from(r"C:\Users\u").join(r"scoop\shims\adb.exe")));
        assert!(c.contains(
            &PathBuf::from(r"C:\Users\u\AppData\Local").join(r"Microsoft\WinGet\Links\adb.exe")
        ));
    }

    #[test]
    fn linux_candidates_include_sdk_home() {
        let env = FakeEnv(HashMap::from([("HOME", "/home/u")]));
        let c = adb_candidates(Os::Linux, &env);
        assert_eq!(c[0], PathBuf::from("/usr/bin/adb"));
        assert!(c.contains(&PathBuf::from("/home/u/Android/Sdk/platform-tools/adb")));
        assert!(c.contains(&PathBuf::from(
            "/home/u/.local/share/flatpak/exports/bin/adb"
        )));
        assert!(c.contains(&PathBuf::from("/var/lib/flatpak/exports/bin/adb")));
    }
}
