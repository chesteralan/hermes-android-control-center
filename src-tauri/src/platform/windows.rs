use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use ::windows::Win32::System::Threading::CREATE_NO_WINDOW;
use process_wrap::tokio::{CommandWrap, CreationFlags, JobObject};

use super::Platform;
use crate::error::{AppError, AppResult};

pub struct WindowsPlatform;

pub async fn prepare_adb_server(program: &Path, args: &[String]) -> AppResult<()> {
    if !program
        .file_stem()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("adb"))
        || args
            .iter()
            .any(|arg| matches!(arg.as_str(), "version" | "start-server" | "kill-server"))
    {
        return Ok(());
    }
    let mut command = tokio::process::Command::new(program);
    command
        .arg("start-server")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut wrapped = CommandWrap::from(command);
    wrapped.wrap(process_wrap::tokio::KillOnDrop);
    wrapped.wrap(CreationFlags(CREATE_NO_WINDOW));
    let child = wrapped.spawn()?;
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        Box::into_pin(child.wait_with_output()),
    )
    .await
    .map_err(|_| AppError::Timeout {
        operation: "Start shared ADB server".into(),
        after_ms: 20_000,
    })??;
    if !result.status.success() {
        return Err(AppError::AdbFailed {
            message: "Could not start the shared ADB server".into(),
            stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
            exit_code: result.status.code(),
        });
    }
    Ok(())
}

impl Platform for WindowsPlatform {
    fn restrict_file(path: &Path) -> AppResult<()> {
        let script = "$ErrorActionPreference='Stop'; $identity=[System.Security.Principal.WindowsIdentity]::GetCurrent().User; $acl=Get-Acl -LiteralPath $env:HACC_SECRET_PATH; $acl.SetAccessRuleProtection($true,$false); foreach ($entry in @($acl.Access)) { $acl.RemoveAccessRuleSpecific($entry) }; $rule=[System.Security.AccessControl.FileSystemAccessRule]::new($identity,[System.Security.AccessControl.FileSystemRights]::FullControl,[System.Security.AccessControl.AccessControlType]::Allow); $acl.AddAccessRule($rule); Set-Acl -LiteralPath $env:HACC_SECRET_PATH -AclObject $acl";
        let output = Command::new("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                script,
            ])
            .env("HACC_SECRET_PATH", path)
            .env_remove("PSModulePath")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW.0)
            .output()?;
        if !output.status.success() {
            return Err(AppError::Io(format!(
                "Could not restrict the secret file ACL to the current Windows user: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }

    fn configure_command(command: &mut CommandWrap) {
        command.wrap(CreationFlags(CREATE_NO_WINDOW));
        command.wrap(JobObject);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_file_acl_contains_only_current_user() {
        let path =
            std::env::temp_dir().join(format!("hacc-secret-'quoted'-{}.tmp", std::process::id()));
        std::fs::write(&path, "").unwrap();
        WindowsPlatform::restrict_file(&path).unwrap();
        let script = "$acl=Get-Acl -LiteralPath $env:HACC_SECRET_PATH; $sid=[System.Security.Principal.WindowsIdentity]::GetCurrent().User; if (!$acl.AreAccessRulesProtected -or $acl.Access.Count -ne 1 -or $acl.Access[0].IdentityReference.Translate([System.Security.Principal.SecurityIdentifier]) -ne $sid) { exit 1 }";
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("HACC_SECRET_PATH", &path)
            .env_remove("PSModulePath")
            .creation_flags(CREATE_NO_WINDOW.0)
            .output()
            .unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(
            output.status.success(),
            "Secret file ACL was not restricted"
        );
    }
}
