# Hermes Control Service

The optional service is installed in Termux and listens on `127.0.0.1:8765` by default. Keep the bind address loopback-only; a non-loopback bind requires a token and exposes the API to the network.

## Install and supervise

Install the Termux packages, then install the service from this directory:

```sh
pkg install python
./install.sh
```

After `termux-services` has started, enable the service with runit:

```sh
sv up hermes-control
sv status hermes-control
```

The run script is installed at `$PREFIX/var/service/hermes-control/run`; runit restarts the process if it exits. The config and environment variables can override the install and config paths.

## Background operation

Android may stop Termux to reclaim memory. Run `termux-wake-lock` while Hermes must remain available, and release it with `termux-wake-unlock` when background operation is no longer needed. The wake lock increases battery use. For reboot startup, install Termux:Boot and add an executable script under `~/.termux/boot/` containing:

```sh
#!/data/data/com.termux/files/usr/bin/sh
termux-wake-lock
sv up hermes-control
```

The service config is created once at `~/.config/hermes-control.toml`; upgrades preserve it.

## Contract tests

Install the service's test extras and run the pytest suite from this directory:

```sh
python3 -m pip install -e '.[test]'
python3 -m pytest
```

The Python responses are checked against schemas generated from the Rust types:

```sh
npm run gen:schemas
```
