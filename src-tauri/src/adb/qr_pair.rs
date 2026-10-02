//! "Pair device with QR code" flow (M2-T6a), mirroring Android Studio's implementation.

use std::time::Duration;

use rand::distr::{Alphanumeric, SampleString};
use serde::Serialize;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use super::client::AdbClient;
use super::types::MdnsServiceKind;
use crate::error::{AppError, AppResult, ErrorPayload};

pub const QR_TIMEOUT: Duration = Duration::from_secs(120);
pub const POLL: Duration = Duration::from_secs(1);
const CONNECT_LOOKUP: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum QrPairEvent {
    Waiting {
        #[ts(type = "number")]
        remaining_ms: u64,
    },
    Found {
        address: String,
    },
    Paired,
    Connected {
        address: String,
    },
    Failed {
        error: ErrorPayload,
    },
    Expired,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QrSession {
    pub session_id: String,
    pub qr_svg: String,
    #[ts(type = "number")]
    pub expires_in_ms: u64,
}

pub struct Credentials {
    pub service_name: String,
    pub password: String,
}

pub fn new_credentials() -> Credentials {
    let mut rng = rand::rng();
    Credentials {
        service_name: format!(
            "hacc-{}",
            Alphanumeric.sample_string(&mut rng, 8).to_lowercase()
        ),
        password: Alphanumeric.sample_string(&mut rng, 12),
    }
}

fn escape(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '\\' | ';' | ',' | ':' | '"' => vec!['\\', c],
            c => vec![c],
        })
        .collect()
}

pub fn qr_payload(c: &Credentials) -> String {
    format!(
        "WIFI:T:ADB;S:{};P:{};;",
        escape(&c.service_name),
        escape(&c.password)
    )
}

pub fn render_svg(payload: &str) -> AppResult<String> {
    let code = qrcode::QrCode::new(payload.as_bytes()).map_err(|e| AppError::Io(e.to_string()))?;
    Ok(code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(240, 240)
        .quiet_zone(true)
        .build())
}

/// Polls mDNS for the phone advertising our service name, pairs, then connects.
pub async fn run(
    client: &AdbClient,
    creds: &Credentials,
    timeout: Duration,
    cancel: CancellationToken,
    emit: impl Fn(QrPairEvent),
) {
    let deadline = Instant::now() + timeout;
    let pairing_addr = loop {
        if cancel.is_cancelled() {
            return;
        }
        let now = Instant::now();
        if now >= deadline {
            emit(QrPairEvent::Expired);
            return;
        }
        emit(QrPairEvent::Waiting {
            remaining_ms: (deadline - now).as_millis() as u64,
        });
        if let Ok(services) = client.mdns_services().await {
            if let Some(s) = services
                .iter()
                .find(|s| s.kind == MdnsServiceKind::Pairing && s.name == creds.service_name)
            {
                break s.address.clone();
            }
        }
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(POLL) => {}
        }
    };
    emit(QrPairEvent::Found {
        address: pairing_addr.clone(),
    });

    if let Err(e) = client.pair(&pairing_addr, &creds.password).await {
        emit(QrPairEvent::Failed {
            error: e.to_payload(),
        });
        return;
    }
    emit(QrPairEvent::Paired);

    let ip = pairing_addr
        .rsplit_once(':')
        .map(|(h, _)| h.to_string())
        .unwrap_or_default();
    let lookup_deadline = Instant::now() + CONNECT_LOOKUP;
    while Instant::now() < lookup_deadline && !cancel.is_cancelled() {
        if let Ok(services) = client.mdns_services().await {
            if let Some(s) = services
                .iter()
                .find(|s| s.kind == MdnsServiceKind::Connect && s.ip == ip)
            {
                // adb may already have auto-connected via mDNS; connect is idempotent.
                match client.connect(&s.address).await {
                    Ok(_) => emit(QrPairEvent::Connected {
                        address: s.address.clone(),
                    }),
                    Err(e) => emit(QrPairEvent::Failed {
                        error: e.to_payload(),
                    }),
                }
                return;
            }
        }
        tokio::time::sleep(POLL).await;
    }
    emit(QrPairEvent::Failed {
        error: AppError::Config(
            "Paired, but the phone's wireless debugging address was not found. Connect with IP and port.".into(),
        )
        .to_payload(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{FakeRunner, RawOutput};
    use std::sync::{Arc, Mutex};

    fn creds() -> Credentials {
        Credentials {
            service_name: "hacc-test1234".into(),
            password: "Secret123abc".into(),
        }
    }

    #[test]
    fn payload_format_and_escaping() {
        assert_eq!(
            qr_payload(&creds()),
            "WIFI:T:ADB;S:hacc-test1234;P:Secret123abc;;"
        );
        assert_eq!(escape("a;b:c"), "a\\;b\\:c");
    }

    #[test]
    fn credentials_are_random_and_alphanumeric() {
        let a = new_credentials();
        let b = new_credentials();
        assert_ne!(a.password, b.password);
        assert!(a.password.chars().all(|c| c.is_ascii_alphanumeric()));
        assert!(a.service_name.starts_with("hacc-"));
    }

    #[test]
    fn renders_svg() {
        let svg = render_svg(&qr_payload(&creds())).unwrap();
        assert!(svg.contains("<svg"));
    }

    fn collect() -> (Arc<Mutex<Vec<QrPairEvent>>>, impl Fn(QrPairEvent)) {
        let events: Arc<Mutex<Vec<QrPairEvent>>> = Arc::default();
        let e = events.clone();
        (events, move |ev| e.lock().unwrap().push(ev))
    }

    #[tokio::test(start_paused = true)]
    async fn full_flow_waiting_found_paired_connected() {
        let f = FakeRunner::new();
        let none = "List of discovered mdns services\n";
        let pairing = "List of discovered mdns services\nhacc-test1234\t_adb-tls-pairing._tcp\t192.0.2.25:40111\n";
        let connect = "List of discovered mdns services\nadb-SER-x\t_adb-tls-connect._tcp\t192.0.2.25:44467\n";
        f.on("mdns services", Ok(RawOutput::ok(none)));
        f.on("mdns services", Ok(RawOutput::ok(pairing)));
        f.on("mdns services", Ok(RawOutput::ok(connect)));
        f.on(
            "pair 192.0.2.25:40111 Secret123abc",
            Ok(RawOutput::ok(
                "Successfully paired to 192.0.2.25:40111 [guid=adb-SER-x]",
            )),
        );
        f.on(
            "connect 192.0.2.25:44467",
            Ok(RawOutput::ok("connected to 192.0.2.25:44467")),
        );
        let client = AdbClient::new("/adb", Arc::new(f));
        let (events, emit) = collect();
        run(
            &client,
            &creds(),
            QR_TIMEOUT,
            CancellationToken::new(),
            emit,
        )
        .await;
        let ev = events.lock().unwrap();
        assert!(matches!(ev[0], QrPairEvent::Waiting { .. }));
        assert!(ev
            .iter()
            .any(|e| matches!(e, QrPairEvent::Found { address } if address == "192.0.2.25:40111")));
        assert!(ev.iter().any(|e| matches!(e, QrPairEvent::Paired)));
        assert!(
            matches!(ev.last().unwrap(), QrPairEvent::Connected { address } if address == "192.0.2.25:44467")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn expires() {
        let f = FakeRunner::new();
        f.on(
            "mdns services",
            Ok(RawOutput::ok("List of discovered mdns services\n")),
        );
        let client = AdbClient::new("/adb", Arc::new(f));
        let (events, emit) = collect();
        run(
            &client,
            &creds(),
            Duration::from_secs(3),
            CancellationToken::new(),
            emit,
        )
        .await;
        assert!(matches!(
            events.lock().unwrap().last().unwrap(),
            QrPairEvent::Expired
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn wrong_password_fails() {
        let f = FakeRunner::new();
        f.on(
            "mdns services",
            Ok(RawOutput::ok(
                "hacc-test1234\t_adb-tls-pairing._tcp\t192.0.2.25:40111\n",
            )),
        );
        f.on(
            "pair 192.0.2.25:40111 Secret123abc",
            Ok(RawOutput::ok(
                "Failed: Wrong password or connection was dropped.",
            )),
        );
        let client = AdbClient::new("/adb", Arc::new(f));
        let (events, emit) = collect();
        run(
            &client,
            &creds(),
            QR_TIMEOUT,
            CancellationToken::new(),
            emit,
        )
        .await;
        assert!(matches!(
            events.lock().unwrap().last().unwrap(),
            QrPairEvent::Failed { .. }
        ));
    }
}
