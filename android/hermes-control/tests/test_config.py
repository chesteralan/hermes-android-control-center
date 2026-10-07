import unittest
import json
from pathlib import Path
from tempfile import TemporaryDirectory

from hermes_control.config import ConfigError, ServiceConfig, load_config, validate_config


class ServiceConfigTests(unittest.TestCase):
    def test_installer_registers_runit_script_without_replacing_config(self) -> None:
        root = Path(__file__).parents[1]
        installer = (root / "install.sh").read_text(encoding="utf-8")
        run_script = root / "service/hermes-control/run"

        self.assertIn('if [ ! -f "$CONFIG" ]', installer)
        self.assertIn("pkg install -y termux-services", installer)
        self.assertIn('cp "$ROOT/service/hermes-control/run" "$SERVICE_DIR/run"', installer)
        self.assertTrue(run_script.is_file())
        self.assertIn("exec \"$APP_HOME/venv/bin/hermes-control\"", run_script.read_text())

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

    def test_action_overlay_is_loaded_without_modifying_toml(self) -> None:
        with TemporaryDirectory() as directory:
            config_path = Path(directory) / "service.toml"
            config_path.write_text('[server]\nport = 8765\n', encoding="utf-8")
            action_path = Path(directory) / "hermes-control-actions.json"
            action_path.write_text(
                json.dumps(
                    {
                        "actions": {"start": "start-script"},
                        "hermes": {
                            "environment": "proot-distro",
                            "distro": "debian",
                            "process_match": "python/hermes",
                        },
                        "logs": {"path": "/root/custom/gateway.log"},
                    }
                ),
                encoding="utf-8",
            )

            config = load_config(config_path)

        self.assertEqual(config.actions, {"start": "start-script"})
        self.assertEqual(config.hermes["process_match"], "python/hermes")
        self.assertEqual(config.log_path, "/root/custom/gateway.log")

    def test_cli_string_config_path_is_accepted(self) -> None:
        config_path = Path(__file__).parents[1] / "hermes-control.toml"
        config = load_config(str(config_path))

        self.assertEqual(config.port, 8765)


if __name__ == "__main__":
    unittest.main()