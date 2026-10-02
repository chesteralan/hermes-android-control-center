//! Validation of user-entered `host:port` addresses before they reach adb.

use std::net::{Ipv4Addr, Ipv6Addr};

use crate::error::{AppError, AppResult};

fn invalid(addr: &str, why: &str) -> AppError {
    AppError::Config(format!(
        "Invalid address \"{addr}\": {why}. Use IP:port, e.g. 192.168.1.25:37145."
    ))
}

/// Accepts `ipv4:port`, `[ipv6]:port` and `hostname:port`.
pub fn validate_address(addr: &str) -> AppResult<()> {
    let addr = addr.trim();
    let (host, port) = if let Some(rest) = addr.strip_prefix('[') {
        let (h, p) = rest
            .split_once("]:")
            .ok_or_else(|| invalid(addr, "missing port"))?;
        h.parse::<Ipv6Addr>()
            .map_err(|_| invalid(addr, "bad IPv6 address"))?;
        (h, p)
    } else {
        let (h, p) = addr
            .rsplit_once(':')
            .ok_or_else(|| invalid(addr, "missing port"))?;
        if h.contains(':') {
            return Err(invalid(addr, "wrap IPv6 addresses in brackets"));
        }
        (h, p)
    };
    let port: u32 = port
        .parse()
        .map_err(|_| invalid(addr, "port must be a number"))?;
    if !(1..=65535).contains(&port) {
        return Err(invalid(addr, "port must be between 1 and 65535"));
    }
    if host.is_empty() {
        return Err(invalid(addr, "missing host"));
    }
    let looks_numeric = host.chars().all(|c| c.is_ascii_digit() || c == '.');
    if looks_numeric && host.parse::<Ipv4Addr>().is_err() {
        return Err(invalid(addr, "bad IPv4 address"));
    }
    let host_ok = host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
        && !host.starts_with('-');
    if !host_ok {
        return Err(invalid(addr, "bad host name"));
    }
    Ok(())
}

pub fn split_host_port(addr: &str) -> Option<(String, u16)> {
    let (h, p) = addr.rsplit_once(':')?;
    let h = h.trim_start_matches('[').trim_end_matches(']');
    Some((h.to_string(), p.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid() {
        for a in [
            "192.168.1.25:37145",
            "[fe80::1]:5555",
            "pixel.local:5555",
            "10.0.0.1:1",
        ] {
            assert!(validate_address(a).is_ok(), "{a}");
        }
    }

    #[test]
    fn rejects_invalid() {
        for a in [
            "",
            "192.168.1.25",
            "192.168.1.25:0",
            "192.168.1.25:70000",
            "999.1.1.1:5555",
            ":5555",
            "fe80::1:5555",
            "host;rm -rf:5555",
            "-s:5555",
        ] {
            assert!(validate_address(a).is_err(), "{a}");
        }
    }

    #[test]
    fn splits() {
        assert_eq!(split_host_port("1.2.3.4:55"), Some(("1.2.3.4".into(), 55)));
        assert_eq!(split_host_port("[fe80::1]:5"), Some(("fe80::1".into(), 5)));
    }
}
