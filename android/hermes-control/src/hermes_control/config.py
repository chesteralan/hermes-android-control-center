from __future__ import annotations

import ipaddress
import os
import tomllib
from dataclasses import dataclass
from pathlib import Path


class ConfigError(ValueError):
    """Invalid or unsafe service configuration."""


@dataclass(frozen=True)
class ServiceConfig:
    host: str
    port: int
    token: str
    allow_arbitrary_commands: bool
    hermes: dict[str, object]
    log_command: str
    log_path: str


def default_config_path() -> Path:
    return Path(os.environ.get("HERMES_CONTROL_CONFIG", "~/.config/hermes-control.toml")).expanduser()


def _section(config: dict[str, object], name: str) -> dict[str, object]:
    value = config.get(name, {})
    if not isinstance(value, dict):
        raise ConfigError(f"[{name}] must be a TOML table.")
    return value


def _is_loopback(host: str) -> bool:
    if host.lower() == "localhost":
        return True
    try:
        return ipaddress.ip_address(host).is_loopback
    except ValueError:
        return False


def validate_config(config: ServiceConfig) -> None:
    if not config.host.strip():
        raise ConfigError("Server bind host cannot be empty.")
    if not isinstance(config.port, int) or isinstance(config.port, bool) or not 1 <= config.port <= 65535:
        raise ConfigError("Server port must be between 1 and 65535.")
    if not _is_loopback(config.host) and not config.token.strip():
        raise ConfigError("Non-loopback bind requires a non-empty control token.")


def load_config(path: Path | None = None) -> ServiceConfig:
    config_path = path or default_config_path()
    try:
        with config_path.open("rb") as source:
            raw = tomllib.load(source)
    except OSError as error:
        raise ConfigError(f"Cannot read service config {config_path}: {error}") from error
    except tomllib.TOMLDecodeError as error:
        raise ConfigError(f"Invalid TOML in {config_path}: {error}") from error

    server = _section(raw, "server")
    hermes = _section(raw, "hermes")
    logs = _section(raw, "logs")
    configured_token = server.get("token", "")
    if not isinstance(configured_token, str):
        raise ConfigError("server.token must be a string.")
    token = os.environ.get("HERMES_CONTROL_TOKEN") or configured_token
    host = server.get("host", "127.0.0.1")
    port = server.get("port", 8765)
    allow_arbitrary_commands = server.get("allow_arbitrary_commands", False)
    log_command = logs.get("command", "tail -n 200 -F /root/.hermes/logs/gateway.log")
    log_path = logs.get("path", "/root/.hermes/logs/gateway.log")
    if not isinstance(host, str):
        raise ConfigError("server.host must be a string.")
    if not isinstance(allow_arbitrary_commands, bool):
        raise ConfigError("server.allow_arbitrary_commands must be a boolean.")
    if not isinstance(log_command, str) or not isinstance(log_path, str):
        raise ConfigError("logs.command and logs.path must be strings.")

    service_config = ServiceConfig(
        host=host,
        port=port,
        token=token,
        allow_arbitrary_commands=allow_arbitrary_commands,
        hermes=hermes,
        log_command=log_command,
        log_path=log_path,
    )
    validate_config(service_config)
    return service_config
