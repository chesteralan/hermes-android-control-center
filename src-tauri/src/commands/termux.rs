use std::time::Duration;

use tauri::State;

use crate::adb::parse::parse_termux;
use crate::error::AppError;
use crate::state::AppState;
use crate::termux::check::{parse_probe, Checklist, TermuxCheck, PROBE};
use crate::termux::keys;
use crate::transport::DeviceTransport;

/// Public key to paste into Termux `~/.ssh/authorized_keys`.
#[tauri::command]
pub async fn get_termux_public_key(state: State<'_, AppState>) -> Result<String, AppError> {
    keys::public_key_line(state.ssh_key()?)
}

/// Walks the Termux bridge step by step and reports what (if anything) needs fixing.
#[tauri::command]
pub async fn check_termux(
    state: State<'_, AppState>,
    serial: String,
) -> Result<TermuxCheck, AppError> {
    let client = state.adb_client().await?;
    let mut c = Checklist::new();

    let pm = client
        .shell(&serial, "pm list packages -i --show-versioncode com.termux; dumpsys package com.termux | grep versionName", Duration::from_secs(10))
        .await?;
    let pkg = parse_termux(&pm.stdout, &pm.stdout);
    if pkg.installed {
        c.ok("termux", "Termux installed", pkg.version_name.clone());
    } else {
        c.fail(
            "termux",
            "Termux installed",
            None,
            "Install Termux from F-Droid or GitHub releases.",
        );
    }

    let port = state.config.read().await.termux.ssh_port;
    if !c.has_failed() {
        match state.forwards.ensure(&client, &serial, port).await {
            Ok(local) => c.ok(
                "forward",
                "Port forward",
                Some(format!("localhost:{local} → phone 127.0.0.1:{port}")),
            ),
            Err(e) => c.fail(
                "forward",
                "Port forward",
                Some(e.user_message()),
                "Reconnect the phone and try again.",
            ),
        }
    } else {
        c.skip("forward", "Port forward");
    }

    let transport = if !c.has_failed() {
        state.ssh.drop_device(&serial).await;
        match state.termux_transport(&serial).await {
            Ok(t) => {
                c.ok("ssh", "SSH into Termux", None);
                Some(t)
            }
            Err(e) => {
                c.fail(
                    "ssh",
                    "SSH into Termux",
                    Some(e.user_message()),
                    "In Termux: pkg install openssh, add the public key to ~/.ssh/authorized_keys, then run sshd.",
                );
                None
            }
        }
    } else {
        c.skip("ssh", "SSH into Termux");
        None
    };

    let mut distros = Vec::new();
    if let Some(t) = transport {
        match t.execute(PROBE, Duration::from_secs(15)).await {
            Ok(out) => {
                let p = parse_probe(&out.stdout);
                match &p.prefix {
                    Some(prefix) => c.ok("prefix", "Termux environment", Some(prefix.clone())),
                    None => c.fail(
                        "prefix",
                        "Termux environment",
                        None,
                        "The SSH session is not running inside Termux.",
                    ),
                }
                if p.proot_distro {
                    let detail = if p.distros.is_empty() {
                        "no distros installed".to_string()
                    } else {
                        p.distros.join(", ")
                    };
                    c.ok("proot", "proot-distro", Some(detail));
                } else {
                    c.skip("proot", "proot-distro (not installed — optional)");
                }
                distros = p.distros;
            }
            Err(e) => c.fail(
                "prefix",
                "Termux environment",
                Some(e.user_message()),
                "Re-run the check.",
            ),
        }
    } else {
        c.skip("prefix", "Termux environment");
    }

    Ok(c.finish(distros))
}

#[tauri::command]
pub async fn forget_termux_host_key(
    state: State<'_, AppState>,
    serial: String,
) -> Result<(), AppError> {
    state.ssh.drop_device(&serial).await;
    state.known_hosts.forget(&state.device_id_for(&serial))
}
