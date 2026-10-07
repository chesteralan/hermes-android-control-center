from __future__ import annotations

import argparse
import asyncio
import hmac
import json
import os
import signal
import time
from pathlib import Path
from typing import Any

from aiohttp import web

from . import __version__
from .config import ConfigError, ServiceConfig, default_config_path, load_config

CONFIG_KEY = web.AppKey("service_config", ServiceConfig)
VERSION_KEY = web.AppKey("version", str)
MAX_TIMEOUT_MS = 600_000
MAX_LOG_TAIL = 2_000


@web.middleware
async def authenticate(request: web.Request, handler: Any) -> web.StreamResponse:
    config = request.app[CONFIG_KEY]
    if config.token:
        supplied = request.headers.get("Authorization", "")
        expected = f"Bearer {config.token}"
        if not hmac.compare_digest(supplied.encode(), expected.encode()):
            raise web.HTTPUnauthorized(headers={"WWW-Authenticate": "Bearer"})
    return await handler(request)


def create_app(config: ServiceConfig) -> web.Application:
    app = web.Application(middlewares=[authenticate])
    app[CONFIG_KEY] = config
    app[VERSION_KEY] = __version__
    app.add_routes(
        [
            web.get("/health", health),
            web.get("/status", status),
            web.post("/command", command),
            web.post("/start", action),
            web.post("/stop", action),
            web.post("/restart", action),
            web.get("/logs", logs),
            web.get("/command/stream", command_stream),
        ]
    )
    return app


async def health(request: web.Request) -> web.Response:
    return web.json_response({"ok": True, "version": request.app[VERSION_KEY]})


async def _run_command(command: str, timeout_ms: int) -> dict[str, object]:
    started = time.monotonic()
    process = await asyncio.create_subprocess_shell(
        command,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        start_new_session=os.name == "posix",
    )
    try:
        stdout, stderr = await asyncio.wait_for(process.communicate(), timeout_ms / 1_000)
    except TimeoutError:
        _kill_process_tree(process)
        stdout, stderr = await process.communicate()
        return {
            "stdout": stdout.decode(errors="replace"),
            "stderr": stderr.decode(errors="replace") + "\nCommand timed out.",
            "exitCode": None,
            "durationMs": round((time.monotonic() - started) * 1_000),
        }
    return {
        "stdout": stdout.decode(errors="replace"),
        "stderr": stderr.decode(errors="replace"),
        "exitCode": process.returncode,
        "durationMs": round((time.monotonic() - started) * 1_000),
    }


def _kill_process_tree(process: asyncio.subprocess.Process) -> None:
    try:
        if os.name == "posix":
            os.killpg(process.pid, signal.SIGKILL)
        elif process.returncode is None:
            process.kill()
    except ProcessLookupError:
        pass


async def _json_body(request: web.Request) -> dict[str, object]:
    try:
        body = await request.json()
    except (json.JSONDecodeError, web.ContentTypeError) as error:
        raise web.HTTPBadRequest(text="Expected a JSON object.") from error
    if not isinstance(body, dict):
        raise web.HTTPBadRequest(text="Expected a JSON object.")
    return body


def _timeout(value: object) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise web.HTTPBadRequest(text="timeout_ms must be a positive integer.")
    return min(value, MAX_TIMEOUT_MS)


async def command(request: web.Request) -> web.Response:
    config = request.app[CONFIG_KEY]
    if not config.allow_arbitrary_commands:
        raise web.HTTPForbidden(text="Arbitrary commands are disabled.")
    body = await _json_body(request)
    value = body.get("command")
    if not isinstance(value, str) or not value.strip():
        raise web.HTTPBadRequest(text="command must be a non-empty string.")
    result = await _run_command(value, _timeout(body.get("timeout_ms", 30_000)))
    return web.json_response(result)


async def _hermes_status(config: ServiceConfig) -> dict[str, object]:
    processes: list[dict[str, object]] = []
    gateway: dict[str, object] | None = None
    probe_succeeded = False
    try:
        process_match = config.hermes.get("process_match", "hermes")
        gateway_match = config.hermes.get("gateway_match", "gateway run")
        if not isinstance(process_match, str) or not isinstance(gateway_match, str):
            process_match, gateway_match = "hermes", "gateway run"
        with os.scandir("/proc") as entries:
            probe_succeeded = True
            for entry in entries:
                if not entry.name.isdecimal():
                    continue
                try:
                    raw = Path(entry.path, "cmdline").read_bytes()
                except OSError:
                    continue
                cmdline = raw.replace(b"\0", b" ").decode(errors="replace").strip()
                if not cmdline or process_match.lower() not in cmdline.lower():
                    continue
                if gateway_match.lower() in cmdline.lower():
                    kind = "gateway"
                elif cmdline.split()[-1:] == ["hermes"]:
                    kind = "cli"
                else:
                    kind = "other"
                item: dict[str, object] = {
                    "pid": int(entry.name),
                    "uptimeSecs": None,
                    "kind": kind,
                    "cmdline": cmdline,
                }
                processes.append(item)
                if kind == "gateway":
                    gateway = item
    except OSError:
        pass

    now = int(time.time())
    supervisor_pid: int | None = None
    try:
        candidate = int((Path.home() / ".hacc/hermes-supervisor.pid").read_text().strip())
        os.kill(candidate, 0)
        supervisor_pid = candidate
    except (OSError, ValueError):
        pass
    recent_restarts = 0
    try:
        recent_lines = (Path.home() / ".hacc/supervisor.log").read_text(errors="replace").splitlines()[-100:]
        recent_restarts = sum(
            1
            for line in recent_lines
            if "restarting in" in line
            and line.split(maxsplit=1)[0].isdigit()
            and now - int(line.split(maxsplit=1)[0]) <= 600
        )
    except OSError:
        pass
    gateway_state = "running" if gateway else "stopped" if probe_succeeded else "unknown"
    warnings = []
    if gateway and supervisor_pid is None:
        warnings.append("Gateway was started outside the app supervisor.")
    if recent_restarts >= 5:
        warnings.append(f"Gateway crash loop: {recent_restarts} restarts in the last 10 minutes.")
    return {
        "gateway": gateway_state,
        "gatewayPid": gateway["pid"] if gateway else None,
        "uptimeSecs": gateway["uptimeSecs"] if gateway else None,
        "pythonVersion": None,
        "processes": processes,
        "platforms": [],
        "supervisor": {
            "running": supervisor_pid is not None,
            "pid": supervisor_pid,
            "recentRestarts": recent_restarts,
        },
        "warnings": warnings if gateway_state != "unknown" else warnings + ["Hermes process state could not be confirmed."],
        "rawStatusOutput": None,
        "source": "api",
        "checkedAt": now,
    }


async def status(_request: web.Request) -> web.Response:
    return web.json_response(await _hermes_status(_request.app[CONFIG_KEY]))


async def action(request: web.Request) -> web.Response:
    config = request.app[CONFIG_KEY]
    action_name = request.path.lstrip("/")
    command_value = config.hermes.get(f"{action_name}_command")
    if not isinstance(command_value, str) or not command_value.strip():
        command_value = config.actions.get(action_name)
    if not isinstance(command_value, str) or not command_value.strip():
        raise web.HTTPServiceUnavailable(text=f"No Hermes {action_name} command is configured.")
    result = await _run_command(command_value, 30_000)
    return web.json_response({"result": result, "status": await _hermes_status(config)})


def _log_line(seq: int, raw: str) -> dict[str, object]:
    return {
        "seq": seq,
        "receivedAt": int(time.time() * 1_000),
        "timestamp": None,
        "level": None,
        "tag": None,
        "message": raw,
        "raw": raw,
    }


def _resolve_log_path(config: ServiceConfig) -> Path:
    configured = str(config.log_path)
    if config.hermes.get("environment") != "proot-distro":
        return Path(configured).expanduser()
    distro = config.hermes.get("distro")
    if not isinstance(distro, str) or not distro:
        return Path(configured).expanduser()
    prefix = Path(os.environ.get("PREFIX", "/data/data/com.termux/files/usr"))
    relative = Path(configured.lstrip("/"))
    if ".." in relative.parts:
        return Path(configured).expanduser()
    roots = (
        prefix / "var/lib/proot-distro/containers" / distro / "rootfs",
        prefix / "var/lib/proot-distro/installed-rootfs" / distro,
    )
    for root in roots:
        if root.is_dir():
            candidate = (root / relative).resolve()
            if candidate.is_relative_to(root.resolve()):
                return candidate
    return Path(configured).expanduser()


async def logs(request: web.Request) -> web.WebSocketResponse:
    config = request.app[CONFIG_KEY]
    try:
        tail = int(request.query.get("tail", "200"))
    except ValueError as error:
        raise web.HTTPBadRequest(text="tail must be an integer.") from error
    if not 0 <= tail <= MAX_LOG_TAIL:
        raise web.HTTPBadRequest(text=f"tail must be between 0 and {MAX_LOG_TAIL}.")
    socket = web.WebSocketResponse(heartbeat=30)
    await socket.prepare(request)
    try:
        with _resolve_log_path(config).open(encoding="utf-8", errors="replace") as source:
            lines = source.readlines()
            offset = source.tell()
        seq = max(0, len(lines) - tail)
        for line in lines[-tail:] if tail else []:
            seq += 1
            await socket.send_json(_log_line(seq, line.rstrip("\r\n")))
        while not socket.closed:
            try:
                message = await asyncio.wait_for(socket.receive(), timeout=0.25)
            except TimeoutError:
                message = None
            if message is not None and message.type in (web.WSMsgType.CLOSE, web.WSMsgType.CLOSED, web.WSMsgType.ERROR):
                break
            try:
                with _resolve_log_path(config).open(encoding="utf-8", errors="replace") as source:
                    source.seek(offset)
                    updates = source.readlines()
                    offset = source.tell()
            except OSError:
                continue
            for line in updates:
                seq += 1
                await socket.send_json(_log_line(seq, line.rstrip("\r\n")))
    except OSError:
        await socket.close(message=b"Log file is unavailable")
    return socket


async def command_stream(request: web.Request) -> web.WebSocketResponse:
    config = request.app[CONFIG_KEY]
    if not config.allow_arbitrary_commands:
        raise web.HTTPForbidden(text="Arbitrary commands are disabled.")
    socket = web.WebSocketResponse()
    await socket.prepare(request)
    message = await socket.receive()
    try:
        body = json.loads(message.data)
    except (json.JSONDecodeError, TypeError):
        body = None
    value = body.get("command") if isinstance(body, dict) else None
    if not isinstance(value, str) or not value.strip():
        await socket.send_json(
            {
                "type": "error",
                "error": {
                    "kind": "config",
                    "message": "command must be a non-empty string.",
                    "details": None,
                },
            }
        )
        await socket.close()
        return socket
    started = time.monotonic()
    process = await asyncio.create_subprocess_shell(
        value,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        start_new_session=os.name == "posix",
    )

    async def forward(stream: asyncio.StreamReader | None, event_type: str) -> None:
        if stream is None:
            return
        while line := await stream.readline():
            await socket.send_json(
                {"type": event_type, "line": line.decode(errors="replace").rstrip("\r\n")}
            )

    async def wait_for_disconnect() -> None:
        while message := await socket.receive():
            if message.type in (web.WSMsgType.CLOSE, web.WSMsgType.CLOSED, web.WSMsgType.ERROR):
                return

    output_task = asyncio.gather(
        forward(process.stdout, "stdout"),
        forward(process.stderr, "stderr"),
        process.wait(),
    )
    disconnect_task = asyncio.create_task(wait_for_disconnect())
    try:
        done, _ = await asyncio.wait(
            (output_task, disconnect_task), return_when=asyncio.FIRST_COMPLETED
        )
        if disconnect_task in done and not output_task.done():
            _kill_process_tree(process)
            await process.wait()
            output_task.cancel()
            await asyncio.gather(output_task, return_exceptions=True)
            return socket
        await output_task
        await socket.send_json(
            {
                "type": "exit",
                "code": process.returncode,
                "durationMs": round((time.monotonic() - started) * 1_000),
            }
        )
        await socket.close()
    except asyncio.CancelledError:
        _kill_process_tree(process)
        await process.wait()
        if not output_task.done():
            output_task.cancel()
        await asyncio.gather(output_task, return_exceptions=True)
        raise
    except Exception:
        _kill_process_tree(process)
        await process.wait()
        if not output_task.done():
            output_task.cancel()
        await asyncio.gather(output_task, return_exceptions=True)
    finally:
        disconnect_task.cancel()
        await asyncio.gather(disconnect_task, return_exceptions=True)
    return socket


def main() -> None:
    parser = argparse.ArgumentParser(description="Hermes Control loopback service")
    parser.add_argument("--config", type=str, default=str(default_config_path()))
    parser.add_argument("--version", action="version", version=__version__)
    args = parser.parse_args()
    try:
        config = load_config(args.config)
    except ConfigError as error:
        parser.error(str(error))
    web.run_app(create_app(config), host=config.host, port=config.port)
