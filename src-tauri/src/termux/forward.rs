//! `adb forward tcp:0 tcp:<remote>` management. Local ports are always OS-assigned (multi-device safe).

use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::adb::{args, AdbClient};
use crate::error::{AppError, AppResult};

pub fn parse_allocated_port(stdout: &str) -> Option<u16> {
    stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())?
        .parse()
        .ok()
}

/// `adb forward --list` lines: `<serial> tcp:<local> tcp:<remote>`.
pub fn list_has(list: &str, serial: &str, local: u16, remote: u16) -> bool {
    let want = [
        serial.to_string(),
        format!("tcp:{local}"),
        format!("tcp:{remote}"),
    ];
    list.lines()
        .any(|l| l.split_whitespace().eq(want.iter().map(String::as_str)))
}

#[derive(Default)]
pub struct ForwardManager {
    ports: Mutex<HashMap<(String, u16), u16>>,
}

impl ForwardManager {
    /// Returns a local port forwarding to `remote` on the device, reusing a live forward.
    pub async fn ensure(&self, client: &AdbClient, serial: &str, remote: u16) -> AppResult<u16> {
        let mut ports = self.ports.lock().await;
        if let Some(local) = ports.get(&(serial.to_string(), remote)).copied() {
            let list = client
                .raw(&["forward".into(), "--list".into()], Duration::from_secs(5))
                .await?;
            if list_has(&list.stdout, serial, local, remote) {
                return Ok(local);
            }
        }
        let out = client
            .raw(
                &args::forward(serial, "tcp:0", &format!("tcp:{remote}")),
                Duration::from_secs(10),
            )
            .await?;
        let local = parse_allocated_port(&out.stdout).ok_or_else(|| AppError::AdbFailed {
            message: format!("could not forward to device port {remote}"),
            stderr: out.combined(),
            exit_code: out.exit_code,
        })?;
        ports.insert((serial.to_string(), remote), local);
        Ok(local)
    }

    pub async fn remove_device(&self, client: &AdbClient, serial: &str) {
        let mut ports = self.ports.lock().await;
        let mine: Vec<(String, u16)> = ports.keys().filter(|(s, _)| s == serial).cloned().collect();
        for key in mine {
            if let Some(local) = ports.remove(&key) {
                let _ = client
                    .raw(
                        &args::forward_remove(serial, &format!("tcp:{local}")),
                        Duration::from_secs(5),
                    )
                    .await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{FakeRunner, RawOutput};
    use std::sync::Arc;

    #[test]
    fn parses_ports_and_list() {
        assert_eq!(parse_allocated_port("41234\n"), Some(41234));
        assert_eq!(parse_allocated_port("error: closed"), None);
        let list = "S tcp:41234 tcp:8022\nOTHER tcp:5 tcp:8022\n";
        assert!(list_has(list, "S", 41234, 8022));
        assert!(!list_has(list, "S", 41234, 8765));
    }

    #[tokio::test]
    async fn allocates_then_reuses_live_forward() {
        let f = FakeRunner::new();
        f.on("-s S forward tcp:0 tcp:8022", Ok(RawOutput::ok("41234\n")));
        f.on(
            "forward --list",
            Ok(RawOutput::ok("S tcp:41234 tcp:8022\n")),
        );
        let client = AdbClient::new("/adb", Arc::new(f.clone()));
        let m = ForwardManager::default();
        assert_eq!(m.ensure(&client, "S", 8022).await.unwrap(), 41234);
        assert_eq!(m.ensure(&client, "S", 8022).await.unwrap(), 41234);
        let allocations = f
            .calls()
            .iter()
            .filter(|c| c.contains(&"tcp:0".to_string()))
            .count();
        assert_eq!(allocations, 1);
    }
}
