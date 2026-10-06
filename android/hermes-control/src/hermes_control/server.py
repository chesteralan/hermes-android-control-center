from __future__ import annotations

import argparse

from aiohttp import web

from . import __version__
from .config import ConfigError, ServiceConfig, default_config_path, load_config

CONFIG_KEY = web.AppKey("service_config", ServiceConfig)


def create_app(config: ServiceConfig) -> web.Application:
    app = web.Application()
    app[CONFIG_KEY] = config
    app[web.AppKey("version", str)] = __version__
    return app


def main() -> None:
    parser = argparse.ArgumentParser(description="Hermes Control loopback service")
    parser.add_argument("--config", type=str, default=str(default_config_path()))
    args = parser.parse_args()
    try:
        config = load_config(args.config)
    except ConfigError as error:
        parser.error(str(error))
    web.run_app(create_app(config), host=config.host, port=config.port)
