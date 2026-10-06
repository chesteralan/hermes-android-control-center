import unittest

from hermes_control.config import ConfigError, ServiceConfig, load_config, validate_config


class ServiceConfigTests(unittest.TestCase):
    def test_sample_config_defaults_to_loopback_and_matches_hermes_defaults(self) -> None:
        from pathlib import Path

        config = load_config(Path(__file__).parents[1] / "hermes-control.toml")

        self.assertEqual(config.host, "127.0.0.1")
        self.assertEqual(config.port, 8765)
        self.assertEqual(config.hermes["gateway_command"], "hermes gateway run")
        self.assertEqual(config.log_path, "/root/.hermes/logs/gateway.log")

    def test_loopback_bind_does_not_require_token(self) -> None:
        validate_config(ServiceConfig("::1", 8765, "", False, {}, "", ""))

    def test_non_loopback_bind_requires_token(self) -> None:
        with self.assertRaisesRegex(ConfigError, "requires a non-empty control token"):
            validate_config(ServiceConfig("0.0.0.0", 8765, "", False, {}, "", ""))

    def test_non_loopback_bind_is_allowed_with_token(self) -> None:
        validate_config(ServiceConfig("0.0.0.0", 8765, "secret", False, {}, "", ""))

    def test_invalid_port_is_rejected(self) -> None:
        with self.assertRaisesRegex(ConfigError, "port must be between"):
            validate_config(ServiceConfig("127.0.0.1", 0, "", False, {}, "", ""))


if __name__ == "__main__":
    unittest.main()