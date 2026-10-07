use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use process_wrap::tokio::{CommandWrap, ProcessGroup};

use super::Platform;

pub struct LinuxPlatform;

impl Platform for LinuxPlatform {
    fn restrict_file(path: &Path) -> crate::error::AppResult<()> {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(())
    }

    fn configure_command(command: &mut CommandWrap) {
        command.wrap(ProcessGroup::leader());
    }
}
