//! Pure argv builders. Every adb invocation in the app is built here.

fn v(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn scoped(serial: &str, rest: &[&str]) -> Vec<String> {
    let mut out = vec!["-s".to_string(), serial.to_string()];
    out.extend(rest.iter().map(|s| s.to_string()));
    out
}

pub fn version() -> Vec<String> {
    v(&["version"])
}

pub fn start_server() -> Vec<String> {
    v(&["start-server"])
}

pub fn devices_l() -> Vec<String> {
    v(&["devices", "-l"])
}

pub fn track_devices() -> Vec<String> {
    v(&["track-devices", "-l"])
}

pub fn connect(address: &str) -> Vec<String> {
    v(&["connect", address])
}

pub fn disconnect(target: &str) -> Vec<String> {
    v(&["disconnect", target])
}

pub fn pair(address: &str, code: &str) -> Vec<String> {
    v(&["pair", address, code])
}

pub fn mdns_check() -> Vec<String> {
    v(&["mdns", "check"])
}

pub fn mdns_services() -> Vec<String> {
    v(&["mdns", "services"])
}

/// `command` is interpreted by the device shell; it is passed as a single argv element.
pub fn shell(serial: &str, command: &str) -> Vec<String> {
    scoped(serial, &["shell", command])
}

pub fn forward(serial: &str, local: &str, remote: &str) -> Vec<String> {
    scoped(serial, &["forward", local, remote])
}

pub fn forward_remove(serial: &str, local: &str) -> Vec<String> {
    scoped(serial, &["forward", "--remove", local])
}

pub fn logcat(serial: &str, filter: &[String]) -> Vec<String> {
    let mut out = scoped(serial, &["logcat", "-v", "threadtime"]);
    out.extend(filter.iter().cloned());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_exact_argv() {
        assert_eq!(devices_l(), ["devices", "-l"]);
        assert_eq!(track_devices(), ["track-devices", "-l"]);
        assert_eq!(connect("1.2.3.4:5555"), ["connect", "1.2.3.4:5555"]);
        assert_eq!(disconnect("1.2.3.4:5555"), ["disconnect", "1.2.3.4:5555"]);
        assert_eq!(
            pair("1.2.3.4:40000", "123456"),
            ["pair", "1.2.3.4:40000", "123456"]
        );
        assert_eq!(mdns_services(), ["mdns", "services"]);
        assert_eq!(mdns_check(), ["mdns", "check"]);
        assert_eq!(version(), ["version"]);
        assert_eq!(start_server(), ["start-server"]);
    }

    #[test]
    fn device_scoped_calls_always_use_serial() {
        let serial = "adb-ABC-x._adb-tls-connect._tcp";
        assert_eq!(
            shell(serial, "getprop ro.serialno"),
            ["-s", serial, "shell", "getprop ro.serialno"]
        );
        assert_eq!(
            forward(serial, "tcp:0", "tcp:8022"),
            ["-s", serial, "forward", "tcp:0", "tcp:8022"]
        );
        assert_eq!(
            forward_remove(serial, "tcp:1234"),
            ["-s", serial, "forward", "--remove", "tcp:1234"]
        );
        assert_eq!(
            logcat(serial, &["*:I".to_string()]),
            ["-s", serial, "logcat", "-v", "threadtime", "*:I"]
        );
    }

    #[test]
    fn shell_command_with_metacharacters_is_one_arg() {
        let a = shell("s", "echo a; rm -rf / && $(x)");
        assert_eq!(a.len(), 4);
        assert_eq!(a[3], "echo a; rm -rf / && $(x)");
    }
}
