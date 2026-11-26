#!/usr/bin/env python3
"""sheprd-machine-agent: Sheprd's Linux Wayland guest agent.

Speaks the bounded 4-byte big-endian length-prefixed JSON protocol over
AF_VSOCK port 7470 (compatible with crates/sheprd-gpui/src/machine/agent/wire.rs
and macos/sheprd-machine-agent/). Only accepts connections from VMADDR_CID_HOST
(CID 2) so guest-local processes cannot borrow session capture or input access.
"""

from __future__ import annotations

import argparse
import json
import os
import signal
import socket
import struct
import subprocess
import sys
import threading
import time
from typing import Any

from wayland import MAX_REPLY_BYTES, Refusal, WaylandBackend

AGENT_PORT = 7470
AGENT_VERSION = 1
VMADDR_CID_HOST = 2
VMADDR_CID_ANY = getattr(socket, "VMADDR_CID_ANY", 0xFFFFFFFF)
MAX_REQUEST_BYTES = 1 << 20
MAX_EXEC_OUTPUT = 1 << 20
DEFAULT_EXEC_TIMEOUT_MS = 120_000
MAX_EXEC_TIMEOUT_MS = 600_000
EXEC_KILL_GRACE_S = 2.0
INPUT_QUEUE_WAIT_S = 30.0
SOCKET_TIMEOUT_S = 30.0


def read_exactly(conn: socket.socket, count: int) -> bytes | None:
    buf = bytearray()
    while len(buf) < count:
        try:
            chunk = conn.recv(count - len(buf))
        except OSError:
            return None
        if not chunk:
            return None
        buf.extend(chunk)
    return bytes(buf)


def send_frame(conn: socket.socket, reply: dict[str, Any]) -> None:
    try:
        body = json.dumps(reply, separators=(",", ":")).encode("utf-8")
        if len(body) > MAX_REPLY_BYTES:
            body = json.dumps(
                Refusal("failed", "reply too large").reply(), separators=(",", ":")
            ).encode("utf-8")
        conn.sendall(struct.pack(">I", len(body)) + body)
    except OSError:
        pass


class _TailBuffer:
    """Drains a pipe on a daemon thread, retaining the last `limit` bytes."""

    def __init__(self, stream: Any, limit: int = MAX_EXEC_OUTPUT) -> None:
        self._limit = limit
        self._lock = threading.Lock()
        self._data = bytearray()
        self._truncated = False
        self._done = threading.Event()
        self._thread = threading.Thread(target=self._drain, args=(stream,), daemon=True)
        self._thread.start()

    def _drain(self, stream: Any) -> None:
        try:
            while True:
                chunk = stream.read(65_536)
                if not chunk:
                    break
                with self._lock:
                    self._data.extend(chunk)
                    if len(self._data) > self._limit:
                        del self._data[: len(self._data) - self._limit]
                        self._truncated = True
        except OSError:
            pass
        finally:
            try:
                stream.close()
            except OSError:
                pass
            self._done.set()

    def wait(self, timeout: float) -> bool:
        return self._done.wait(max(0.0, timeout))

    def snapshot(self) -> tuple[str, bool]:
        with self._lock:
            return self._data.decode("utf-8", errors="replace"), self._truncated


def resolve_cwd(cwd: Any) -> str:
    home = os.path.expanduser("~")
    if cwd is None:
        return home
    if not isinstance(cwd, str) or not cwd:
        raise Refusal("invalid", "cwd must be a non-empty string")
    expanded = home if cwd == "~" else os.path.join(home, cwd[2:]) if cwd.startswith("~/") else cwd
    if not os.path.isdir(expanded):
        raise Refusal("invalid", f"no directory {expanded}")
    return expanded


def run_exec(request: dict[str, Any], env: dict[str, str]) -> dict[str, Any]:
    argv = request.get("argv")
    if (
        not isinstance(argv, list)
        or not argv
        or not all(isinstance(arg, str) for arg in argv)
        or not argv[0]
    ):
        raise Refusal("invalid", "exec needs argv")
    raw_timeout = request.get("timeout_ms", DEFAULT_EXEC_TIMEOUT_MS)
    if isinstance(raw_timeout, bool) or not isinstance(raw_timeout, int) or raw_timeout <= 0:
        raise Refusal("invalid", "timeout_ms must be a positive integer")
    timeout_s = min(raw_timeout, MAX_EXEC_TIMEOUT_MS) / 1000.0
    directory = resolve_cwd(request.get("cwd"))

    exec_env = dict(env)
    exec_env.setdefault("PATH", "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin")
    try:
        proc = subprocess.Popen(
            argv,
            cwd=directory,
            env=exec_env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
        )
    except OSError as exc:
        raise Refusal("failed", f"cannot run {argv[0]}: {exc}") from exc

    stdout = _TailBuffer(proc.stdout)
    stderr = _TailBuffer(proc.stderr)
    deadline = time.monotonic() + timeout_s
    timed_out = False
    try:
        returncode = proc.wait(timeout=timeout_s)
    except subprocess.TimeoutExpired:
        timed_out = True
        _kill_group(proc.pid, signal.SIGTERM)
        try:
            returncode = proc.wait(timeout=EXEC_KILL_GRACE_S)
        except subprocess.TimeoutExpired:
            returncode = None
        # Always SIGKILL the whole process group even if the leader exited on
        # SIGTERM: a surviving child ignoring SIGTERM keeps the group alive.
        _kill_group(proc.pid, signal.SIGKILL)
        if returncode is None:
            returncode = proc.wait()

    now = time.monotonic()
    drain_deadline = now + EXEC_KILL_GRACE_S if now > deadline else deadline
    drained = stdout.wait(drain_deadline - time.monotonic()) and stderr.wait(
        drain_deadline - time.monotonic()
    )
    if not drained:
        _kill_group(proc.pid, signal.SIGKILL)
        kill_deadline = time.monotonic() + EXEC_KILL_GRACE_S
        stdout.wait(kill_deadline - time.monotonic())
        stderr.wait(kill_deadline - time.monotonic())

    out_text, out_cut = stdout.snapshot()
    err_text, err_cut = stderr.snapshot()
    reply: dict[str, Any] = {
        "stdout": out_text,
        "stderr": err_text,
        "truncated": out_cut or err_cut,
        "timed_out": timed_out,
    }
    if returncode >= 0:
        reply["status"] = returncode
    else:
        reply["signal"] = -returncode
    return reply


def _kill_group(pid: int, sig: int) -> None:
    try:
        os.killpg(pid, sig)
    except ProcessLookupError:
        pass
    except OSError:
        try:
            os.kill(pid, sig)
        except OSError:
            pass


class AgentServer:
    """Dispatches framed requests with serialized input and graceful shutdown."""

    def __init__(self, backend: WaylandBackend | None = None) -> None:
        self.backend = backend or WaylandBackend()
        self._input_lock = threading.Lock()
        self._state_lock = threading.Lock()
        self._active = 0
        self._stopping = False
        self._drained = threading.Event()
        self._drained.set()

    def request_stop(self) -> None:
        with self._state_lock:
            self._stopping = True

    def wait_drained(self, timeout: float = 5.0) -> bool:
        return self._drained.wait(timeout)

    def _begin(self) -> bool:
        with self._state_lock:
            if self._stopping:
                return False
            self._active += 1
            self._drained.clear()
            return True

    def _end(self) -> None:
        with self._state_lock:
            self._active -= 1
            if self._active == 0:
                self._drained.set()

    def _posting(self, fn: Any) -> dict[str, Any]:
        if not self._input_lock.acquire(timeout=INPUT_QUEUE_WAIT_S):
            raise Refusal("failed", "other input is still being posted; try again when it is done")
        try:
            return fn()
        finally:
            self._input_lock.release()

    def handle(self, request: dict[str, Any]) -> dict[str, Any]:
        op = request.get("op")
        if op == "hello":
            return self.backend.hello(AGENT_VERSION)
        if op == "exec":
            return run_exec(request, self.backend.session_env())
        if op == "screenshot":
            return self.backend.screenshot()
        if op == "click":
            return self._posting(lambda: self.backend.click(request))
        if op == "type":
            return self._posting(lambda: self.backend.type_text(request))
        if op == "key":
            return self._posting(lambda: self.backend.key(request))
        raise Refusal("invalid", "unknown op")

    def serve_connection(self, conn: socket.socket) -> None:
        try:
            conn.settimeout(SOCKET_TIMEOUT_S)
            header = read_exactly(conn, 4)
            if header is None:
                return
            (length,) = struct.unpack(">I", header)
            if length == 0 or length > MAX_REQUEST_BYTES:
                send_frame(conn, Refusal("invalid", "request length out of bounds").reply())
                return
            body = read_exactly(conn, length)
            if body is None:
                return
            if not self._begin():
                send_frame(
                    conn,
                    Refusal("failed", "Sheprd Agent is restarting; try again in a moment").reply(),
                )
                return
            try:
                try:
                    decoded = json.loads(body.decode("utf-8"))
                except (UnicodeDecodeError, json.JSONDecodeError) as exc:
                    raise Refusal("invalid", f"invalid JSON request: {exc}") from exc
                if not isinstance(decoded, dict):
                    raise Refusal("invalid", "a request is a JSON object")
                reply = self.handle(decoded)
                reply["ok"] = True
            except Refusal as refusal:
                reply = refusal.reply()
            except Exception as exc:  # pragma: no cover - defensive catch-all
                reply = Refusal("failed", str(exc)).reply()
            try:
                send_frame(conn, reply)
            finally:
                self._end()
        finally:
            try:
                conn.close()
            except OSError:
                pass


def is_host_peer(addr: Any) -> bool:
    """Returns True only when a vsock peer address is VMADDR_CID_HOST (CID 2)."""
    return isinstance(addr, tuple) and len(addr) >= 1 and addr[0] == VMADDR_CID_HOST


def serve_vsock(port: int, server: AgentServer) -> int:
    if not hasattr(socket, "AF_VSOCK"):
        print("AF_VSOCK is not supported on this platform", file=sys.stderr, flush=True)
        return 1
    sock = socket.socket(socket.AF_VSOCK, socket.SOCK_STREAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind((VMADDR_CID_ANY, port))
    sock.listen(16)
    sock.settimeout(1.0)
    print(f"listening on vsock port {port}", flush=True)

    def _on_signal(_signum: int, _frame: Any) -> None:
        server.request_stop()

    signal.signal(signal.SIGTERM, _on_signal)
    signal.signal(signal.SIGINT, _on_signal)

    try:
        while not server._stopping:
            try:
                conn, peer = sock.accept()
            except socket.timeout:
                continue
            except OSError:
                if server._stopping:
                    break
                continue
            if not is_host_peer(peer):
                try:
                    conn.close()
                except OSError:
                    pass
                continue
            threading.Thread(target=server.serve_connection, args=(conn,), daemon=True).start()
    finally:
        try:
            sock.close()
        except OSError:
            pass
        server.wait_drained(5.0)
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Sheprd Linux Wayland guest agent")
    parser.add_argument("--port", type=int, default=AGENT_PORT, help="vsock port to listen on")
    args = parser.parse_args()
    return serve_vsock(args.port, AgentServer())


if __name__ == "__main__":
    raise SystemExit(main())
