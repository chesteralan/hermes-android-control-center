//! Dev probe for the Termux bridge against a real phone (not run in CI).
//! Usage: cargo run --example termux_probe -- <serial> [command]

use std::sync::Arc;
use std::time::Duration;

use hacc_lib::adb::AdbClient;
use hacc_lib::process::TokioRunner;
use hacc_lib::termux::{forward::ForwardManager, keys, known_hosts::KnownHosts, ssh};
use hacc_lib::transport::DeviceTransport;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let serial = args.next().expect("usage: termux_probe <serial> [command]");
    let command = args.next().unwrap_or_else(|| "whoami; echo $PREFIX".into());
    let data_dir = dirs_data_dir();
    let client = AdbClient::new("adb", Arc::new(TokioRunner));
    let port = ForwardManager::default()
        .ensure(&client, &serial, 8022)
        .await;
    println!("forward: {port:?}");
    let key = keys::load_or_create(&data_dir).expect("key");
    println!("public key: {}", keys::public_key_line(&key).unwrap());
    let kh = KnownHosts::load(&data_dir);
    let handle = ssh::connect(port.expect("forward"), "termux", &key, &kh, &serial).await;
    match handle {
        Err(e) => println!("connect error: {} | {:?}", e.user_message(), e),
        Ok(h) => {
            let t = ssh::TermuxSshTransport::new(h);
            let secs = std::env::var("PROBE_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(15);
            match t.execute(&command, Duration::from_secs(secs)).await {
                Ok(r) => println!(
                    "exit={:?} {}ms\n{}{}",
                    r.exit_code, r.duration_ms, r.stdout, r.stderr
                ),
                Err(e) => println!("{e:?}"),
            }
        }
    }
}

fn dirs_data_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").expect("HOME");
    std::path::PathBuf::from(home).join("Library/Application Support/com.hermes.controlcenter")
}
