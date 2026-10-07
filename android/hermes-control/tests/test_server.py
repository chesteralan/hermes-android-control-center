import asyncio
import json
import os
import tempfile
from pathlib import Path
from unittest import IsolatedAsyncioTestCase
from unittest.mock import AsyncMock, patch

from aiohttp.test_utils import TestClient, TestServer
from jsonschema import validate

from hermes_control.config import ServiceConfig
from hermes_control.server import _resolve_log_path, create_app


def service_config(*, allow_commands: bool = True, log_path: str = "") -> ServiceConfig:
    return ServiceConfig(
        host="127.0.0.1",
        port=8765,
        token="test-token",
        allow_arbitrary_commands=allow_commands,
        hermes={"start_command": "start", "stop_command": "stop", "restart_command": "restart"},
        log_command="",
        log_path=log_path,
    )


def validate_contract(name: str, body: object) -> None:
    schema_path = Path(__file__).parents[1] / "schema" / f"{name}.schema.json"
    validate(body, json.loads(schema_path.read_text(encoding="utf-8")))


class ControlApiTests(IsolatedAsyncioTestCase):
    async def asyncSetUp(self) -> None:
        self.client = TestClient(TestServer(create_app(service_config())))
        await self.client.start_server()
        self.auth = {"Authorization": "Bearer test-token"}

    async def asyncTearDown(self) -> None:
        await self.client.close()

    async def test_health_requires_configured_bearer_token(self) -> None:
        unauthorized = await self.client.get("/health")
        self.assertEqual(unauthorized.status, 401)
        response = await self.client.get("/health", headers=self.auth)
        self.assertEqual(response.status, 200)
        self.assertEqual(await response.json(), {"ok": True, "version": "0.1.0"})

    async def test_status_uses_hermes_status_contract(self) -> None:
        with patch(
            "hermes_control.server._hermes_status",
            new=AsyncMock(
                return_value={
                    "gateway": "stopped",
                    "gatewayPid": None,
                    "uptimeSecs": None,
                    "pythonVersion": None,
                    "processes": [],
                    "platforms": [],
                    "supervisor": {"running": False, "pid": None, "recentRestarts": 0},
                    "warnings": [],
                    "rawStatusOutput": None,
                    "source": "termuxSsh",
                    "checkedAt": 1,
                }
            ),
        ):
            response = await self.client.get("/status", headers=self.auth)
        body = await response.json()
        self.assertEqual(response.status, 200)
        self.assertEqual(body["gateway"], "stopped")
        self.assertIn("supervisor", body)
        self.assertIn("checkedAt", body)
        validate_contract("HermesStatus", body)

    async def test_live_status_probe_matches_rust_schema(self) -> None:
        response = await self.client.get("/status", headers=self.auth)
        self.assertEqual(response.status, 200)
        body = await response.json()
        self.assertEqual(body["source"], "api")
        validate_contract("HermesStatus", body)

    async def test_arbitrary_command_is_disabled_by_default(self) -> None:
        client = TestClient(TestServer(create_app(service_config(allow_commands=False))))
        await client.start_server()
        try:
            response = await client.post(
                "/command", headers=self.auth, json={"command": "echo unsafe"}
            )
            self.assertEqual(response.status, 403)
        finally:
            await client.close()

    async def test_command_returns_command_result_shape(self) -> None:
        expected = {"stdout": "ok\n", "stderr": "", "exitCode": 0, "durationMs": 4}
        with patch("hermes_control.server._run_command", new=AsyncMock(return_value=expected)) as run:
            response = await self.client.post(
                "/command",
                headers=self.auth,
                json={"command": "echo ok", "timeout_ms": 5000},
            )
        self.assertEqual(response.status, 200)
        self.assertEqual(await response.json(), expected)
        run.assert_awaited_once_with("echo ok", 5000)
        validate_contract("CommandResult", expected)

    async def test_action_returns_command_result_and_status(self) -> None:
        result = {"stdout": "", "stderr": "", "exitCode": 0, "durationMs": 1}
        current_status = {"gateway": "running", "checkedAt": 1}
        with (
            patch("hermes_control.server._run_command", new=AsyncMock(return_value=result)),
            patch(
                "hermes_control.server._hermes_status",
                new=AsyncMock(return_value=current_status),
            ),
        ):
            response = await self.client.post("/start", headers=self.auth)
        self.assertEqual(await response.json(), {"result": result, "status": current_status})

    async def test_start_stop_and_restart_routes_dispatch_configured_actions(self) -> None:
        result = {"stdout": "ok", "stderr": "", "exitCode": 0, "durationMs": 1}
        current_status = {"gateway": "stopped", "checkedAt": 1}
        with (
            patch("hermes_control.server._run_command", new=AsyncMock(return_value=result)) as run,
            patch(
                "hermes_control.server._hermes_status",
                new=AsyncMock(return_value=current_status),
            ),
        ):
            for action_name in ("start", "stop", "restart"):
                response = await self.client.post(f"/{action_name}", headers=self.auth)
                self.assertEqual(response.status, 200)
                self.assertEqual((await response.json())["status"], current_status)
        self.assertEqual(run.await_count, 3)

    async def test_action_overlay_supplies_missing_toml_commands(self) -> None:
        config = service_config()
        config.hermes["start_command"] = ""
        config.actions["start"] = "overlay start"
        client = TestClient(TestServer(create_app(config)))
        await client.start_server()
        with (
            patch(
                "hermes_control.server._run_command",
                new=AsyncMock(return_value={"stdout": "", "stderr": "", "exitCode": 0, "durationMs": 1}),
            ) as run,
            patch("hermes_control.server._hermes_status", new=AsyncMock(return_value={"gateway": "stopped"})),
        ):
            await client.post("/start", headers=self.auth)
        run.assert_awaited_once_with("overlay start", 30_000)
        await client.close()

    async def test_logs_websocket_sends_requested_tail(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "gateway.log"
            path.write_text("first\nsecond\nthird\n", encoding="utf-8")
            client = TestClient(TestServer(create_app(service_config(log_path=str(path)))))
            await client.start_server()
            try:
                socket = await client.ws_connect("/logs?tail=2", headers=self.auth)
                lines = [await socket.receive_json(), await socket.receive_json()]
                self.assertEqual([line["raw"] for line in lines], ["second", "third"])
                self.assertTrue(all("receivedAt" in line and "message" in line for line in lines))
                for line in lines:
                    validate_contract("LogLine", line)
                await socket.close()
            finally:
                await client.close()

    async def test_log_path_resolves_inside_both_proot_distro_layouts(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            prefix = Path(directory)
            root = prefix / "var/lib/proot-distro/containers/debian/rootfs"
            root.mkdir(parents=True)
            with patch.dict(os.environ, {"PREFIX": str(prefix)}):
                config = service_config(log_path="/root/.hermes/logs/gateway.log")
                config.hermes.update({"environment": "proot-distro", "distro": "debian"})
                self.assertEqual(
                    _resolve_log_path(config),
                    (root / "root/.hermes/logs/gateway.log").resolve(),
                )

                installed_root = prefix / "var/lib/proot-distro/installed-rootfs/debian"
                installed_root.parent.mkdir(parents=True)
                root.rename(installed_root)
                self.assertEqual(
                    _resolve_log_path(config),
                    (installed_root / "root/.hermes/logs/gateway.log").resolve(),
                )

    async def test_command_stream_sends_output_and_exit(self) -> None:
        socket = await self.client.ws_connect("/command/stream", headers=self.auth)
        await socket.send_json({"command": "printf streamed"})
        event = await socket.receive_json()
        exit_event = await socket.receive_json()
        self.assertEqual(event, {"type": "stdout", "line": "streamed"})
        self.assertEqual(exit_event["type"], "exit")
        self.assertEqual(exit_event["code"], 0)
        validate_contract("StreamEvent", event)
        validate_contract("StreamEvent", exit_event)
        await socket.close()

    async def test_command_stream_is_rejected_before_upgrade_when_disabled(self) -> None:
        client = TestClient(TestServer(create_app(service_config(allow_commands=False))))
        await client.start_server()
        try:
            with self.assertRaises(Exception):
                await client.ws_connect("/command/stream", headers=self.auth)
        finally:
            await client.close()

    async def test_command_stream_disconnect_terminates_process_group(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            pid_path = Path(directory) / "command.pid"
            socket = await self.client.ws_connect("/command/stream", headers=self.auth)
            command = f"echo $$ > {pid_path}; while :; do echo running; sleep 0.02; done"
            await socket.send_json({"command": command})
            await asyncio.wait_for(socket.receive_json(), timeout=2)
            process_id = int(pid_path.read_text(encoding="utf-8"))
            await socket.close()
            for _ in range(100):
                try:
                    os.kill(process_id, 0)
                except ProcessLookupError:
                    break
                await asyncio.sleep(0.02)
            else:
                self.fail("streamed command process survived WebSocket disconnect")


if __name__ == "__main__":
    import unittest

    unittest.main()