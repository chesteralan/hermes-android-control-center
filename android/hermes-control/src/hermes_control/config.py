from __future__ import annotations

import ipaddress
import json
import os
import tomllib
from dataclasses import dataclass, field
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
    actions: dict[str, str] = field(default_factory=dict)


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


def load_config(path: Path | str | None = None) -> ServiceConfig:
    config_path = Path(path).expanduser() if path is not None else default_config_path()
    try:
        with config_path.open("rb") as source:
            raw = tomllib.load(source)
    except OSError as error:
        raise ConfigError(f"Cannot read service config {config_path}: {error}") from error
    except tomllib.TOMLDecodeError as error:
        raise ConfigError(f"Invalid TOML in {config_path}: {error}") from error

    actions_path = Path(
        os.environ.get("HERMES_CONTROL_ACTIONS", str(config_path.with_name("hermes-control-actions.json")))
    ).expanduser()
    try:
        with actions_path.open(encoding="utf-8") as source:
            overlay = json.load(source)
    except FileNotFoundError:
        overlay = {}
    except (OSError, json.JSONDecodeError) as error:
        raise ConfigError(f"Cannot read action config {actions_path}: {error}") from error
    if not isinstance(overlay, dict):
        raise ConfigError("Action config must be a JSON object.")
    if set(overlay).issubset({"start", "stop", "restart"}):
        actions = overlay
        hermes_overrides: dict[str, str] = {}
        logs_overrides: dict[str, str] = {}
    else:
        if set(overlay) - {"actions", "hermes", "logs"}:
            raise ConfigError("Action config contains unsupported fields.")
        actions = overlay.get("actions", {})
        hermes_overrides = overlay.get("hermes", {})
        logs_overrides = overlay.get("logs", {})
    if not isinstance(actions, dict) or any(
        key not in {"start", "stop", "restart"} or not isinstance(value, str)
        for key, value in actions.items()
    ):
        raise ConfigError("Action config must map start, stop, and restart to strings.")
    if not isinstance(hermes_overrides, dict) or any(
        key not in {"process_match", "gateway_match", "environment", "distro"}
        or not isinstance(value, str)
        for key, value in hermes_overrides.items()
    ):
        raise ConfigError("Hermes action config contains invalid process settings.")
    if not isinstance(logs_overrides, dict) or any(
        key != "path" or not isinstance(value, str) for key, value in logs_overrides.items()
    ):
        raise ConfigError("Hermes action config contains invalid log settings.")

    server = _section(raw, "server")
    hermes = _section(raw, "hermes")
    hermes.update(hermes_overrides)
    logs = _section(raw, "logs")
    logs.update(logs_overrides)
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
        actions=actions,
    )
    validate_config(service_config)
    return service_config
