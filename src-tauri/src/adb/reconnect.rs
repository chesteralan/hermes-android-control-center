//! Reconnect a lost wireless device with backoff (and mDNS rediscovery of a changed port).

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::address::split_host_port;
use super::backoff::Backoff;
use super::client::AdbClient;
use super::parse::{device_id_from_serial, mdns_instance_serialno};
use super::types::{ConnectionType, DeviceState, MdnsServiceKind};
use crate::devices::{ReconnectPhase, ReconnectStatus};

#[derive(Debug, Clone)]
pub struct ReconnectTarget {
    pub serial: String,
    pub device_id: Option<String>,
    pub connection: ConnectionType,
    /// `ip:port` to try first (for `WirelessIp` devices).
    pub address: Option<String>,
    pub ip: Option<String>,
}

impl ReconnectTarget {
    pub fn from_device(d: &crate::adb::AndroidDevice) -> Self {
        let address = (d.connection == ConnectionType::WirelessIp).then(|| d.serial.clone());
        Self {
            serial: d.serial.clone(),
            device_id: d
                .device_id
                .clone()
                .or_else(|| device_id_from_serial(&d.serial)),
            connection: d.connection,
            ip: d
                .ip_address
                .clone()
                .or_else(|| address.as_deref().and_then(split_host_port).map(|(h, _)| h)),
            address,
        }
    }
}

/// Returns true when the device is back.
pub async fn run(
    client: &AdbClient,
    target: ReconnectTarget,
    schedule_ms: &[u64],
    cancel: CancellationToken,
    emit: impl Fn(ReconnectStatus),
) -> bool {
    let mut backoff = Backoff::new(schedule_ms);
    let max = backoff.max_attempts() as u32;
    let mut address = target.address.clone();
    let status = |phase, attempt: u32, next: Option<Duration>| ReconnectStatus {
        serial: target.serial.clone(),
        device_id: target.device_id.clone(),
        phase,
        attempt,
        max_attempts: max,
        next_delay_ms: next.map(|d| d.as_millis() as u64),
    };

    while let Some(delay) = backoff.next_delay() {
        let attempt = backoff.attempt() as u32;
        emit(status(ReconnectPhase::Waiting, attempt, Some(delay)));
        tokio::select! {
            _ = cancel.cancelled() => return false,
            _ = tokio::time::sleep(delay) => {}
        }
        emit(status(ReconnectPhase::Attempting, attempt, None));

        if is_back(client, &target).await {
            emit(status(ReconnectPhase::Connected, attempt, None));
            return true;
        }

        // From the 2nd attempt (or when we have no address), look for the device via mDNS:
        // the wireless debugging port changes whenever it is toggled.
        if attempt >= 2 || address.is_none() {
            if let Some(found) = rediscover(client, &target).await {
                address = Some(found);
            }
        }
        if let Some(addr) = &address {
            if client.connect(addr).await.is_ok() && is_back(client, &target).await {
                emit(status(ReconnectPhase::Connected, attempt, None));
                return true;
            }
        }
    }
    emit(status(ReconnectPhase::GaveUp, max, None));
    false
}

async fn is_back(client: &AdbClient, target: &ReconnectTarget) -> bool {
    let Ok(devices) = client.devices().await else {
        return false;
    };
    devices.iter().any(|d| {
        d.state == DeviceState::Device
            && (d.serial == target.serial
                || target.device_id.is_some()
                    && device_id_from_serial(&d.serial) == target.device_id
                || target.ip.is_some() && split_host_port(&d.serial).map(|(h, _)| h) == target.ip)
    })
}

async fn rediscover(client: &AdbClient, target: &ReconnectTarget) -> Option<String> {
    let services = client.mdns_services().await.ok()?;
    let connect = services
        .iter()
        .filter(|s| s.kind == MdnsServiceKind::Connect);
    let by_id = target.device_id.as_ref().and_then(|id| {
        connect
            .clone()
            .find(|s| mdns_instance_serialno(&s.name).as_ref() == Some(id))
    });
    let by_ip = || {
        target
            .ip
            .as_ref()
            .and_then(|ip| connect.clone().find(|s| &s.ip == ip))
    };
    by_id.or_else(by_ip).map(|s| s.address.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{FakeRunner, RawOutput};
    use std::sync::{Arc, Mutex};

    fn target() -> ReconnectTarget {
        ReconnectTarget {
            serial: "192.0.2.10:5555".into(),
            device_id: Some("SER1".into()),
            connection: ConnectionType::WirelessIp,
            address: Some("192.0.2.10:5555".into()),
            ip: Some("192.0.2.10".into()),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn gives_up_after_schedule() {
        let f = FakeRunner::new();
        f.on(
            "devices -l",
            Ok(RawOutput::ok("List of devices attached\n\n")),
        );
        f.on(
            "connect 192.0.2.10:5555",
            Ok(RawOutput::ok(
                "failed to connect to '192.0.2.10:5555': Connection refused",
            )),
        );
        f.on(
            "mdns services",
            Ok(RawOutput::ok("List of discovered mdns services\n")),
        );
        let client = AdbClient::new("/adb", Arc::new(f));
        let events = Arc::new(Mutex::new(Vec::new()));
        let ev = events.clone();
        let ok = run(
            &client,
            target(),
            &[1_000, 2_000],
            CancellationToken::new(),
            move |s| ev.lock().unwrap().push(s),
        )
        .await;
        assert!(!ok);
        let ev = events.lock().unwrap();
        assert_eq!(ev.last().unwrap().phase, ReconnectPhase::GaveUp);
        let waits: Vec<_> = ev
            .iter()
            .filter(|e| e.phase == ReconnectPhase::Waiting)
            .map(|e| e.next_delay_ms)
            .collect();
        assert_eq!(waits, vec![Some(1_000), Some(2_000)]);
    }

    #[tokio::test(start_paused = true)]
    async fn rediscovers_new_port_via_mdns() {
        let f = FakeRunner::new();
        f.on(
            "devices -l",
            Ok(RawOutput::ok("List of devices attached\n\n")),
        );
        f.on(
            "devices -l",
            Ok(RawOutput::ok("List of devices attached\n\n")),
        );
        f.on(
            "devices -l",
            Ok(RawOutput::ok("List of devices attached\n\n")),
        );
        f.on(
            "devices -l",
            Ok(RawOutput::ok(
                "List of devices attached\n192.0.2.10:41000 device\n",
            )),
        );
        f.on(
            "connect 192.0.2.10:5555",
            Ok(RawOutput::ok(
                "failed to connect to '192.0.2.10:5555': Connection refused",
            )),
        );
        f.on(
            "mdns services",
            Ok(RawOutput::ok("List of discovered mdns services\nadb-SER1-abc\t_adb-tls-connect._tcp\t192.0.2.10:41000\n")),
        );
        f.on(
            "connect 192.0.2.10:41000",
            Ok(RawOutput::ok("connected to 192.0.2.10:41000")),
        );
        let mut t = target();
        t.serial = "192.0.2.10:5555".into();
        let client = AdbClient::new("/adb", Arc::new(f.clone()));
        let ok = run(
            &client,
            t,
            &[1_000, 2_000, 5_000],
            CancellationToken::new(),
            |_| {},
        )
        .await;
        assert!(f
            .calls()
            .iter()
            .any(|c| c == &["connect", "192.0.2.10:41000"]));
        assert!(ok);
    }

    #[tokio::test(start_paused = true)]
    async fn succeeds_when_device_returns_on_its_own() {
        let f = FakeRunner::new();
        f.on(
            "devices -l",
            Ok(RawOutput::ok(
                "List of devices attached\n192.0.2.10:5555 device\n",
            )),
        );
        let client = AdbClient::new("/adb", Arc::new(f));
        assert!(
            run(
                &client,
                target(),
                &[1_000],
                CancellationToken::new(),
                |_| {}
            )
            .await
        );
    }

    #[tokio::test(start_paused = true)]
    async fn cancel_stops_immediately() {
        let f = FakeRunner::new();
        let client = AdbClient::new("/adb", Arc::new(f.clone()));
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(!run(&client, target(), &[1_000], cancel, |_| {}).await);
        assert!(f.calls().is_empty());
    }
}
