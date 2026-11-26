"""Unit tests for the Linux Wayland guest agent protocol, capture, input, and recovery."""

from __future__ import annotations

import base64
import json
import os
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import agent  # noqa: E402
import wayland  # noqa: E402


def make_png(width: int, height: int) -> bytes:
    ihdr_data = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    ihdr = struct.pack(">I", len(ihdr_data)) + b"IHDR" + ihdr_data + b"\x00\x00\x00\x00"
    return wayland.PNG_SIGNATURE + ihdr


def exchange(server: agent.AgentServer, payload: dict) -> dict:
    client, peer = socket.socketpair()
    thread = threading.Thread(target=server.serve_connection, args=(peer,), daemon=True)
    thread.start()
    try:
        raw = json.dumps(payload).encode("utf-8")
        client.sendall(struct.pack(">I", len(raw)) + raw)
        header = agent.read_exactly(client, 4)
        assert header is not None
        (length,) = struct.unpack(">I", header)
        body = agent.read_exactly(client, length)
        assert body is not None
        return json.loads(body.decode("utf-8"))
    finally:
        client.close()
        thread.join(timeout=5.0)


class StubWaylandBackend(wayland.WaylandBackend):
    """Deterministic fake Wayland backend for testing without a live compositor."""

    def __init__(
        self,
        *,
        desktop_ready: bool = True,
        width: int = 1280,
        height: int = 800,
        scale: float = 1.0,
        has_swaymsg: bool = True,
        has_grim: bool = True,
        has_wtype: bool = True,
        png_bytes: bytes | None = None,
    ) -> None:
        super().__init__(base_env={"PATH": os.environ.get("PATH", "/usr/bin:/bin")})
        self.desktop_ready = desktop_ready
        self.width = width
        self.height = height
        self.scale = scale
        self.has_swaymsg = has_swaymsg
        self.has_grim = has_grim
        self.has_wtype = has_wtype
        self.png_bytes = png_bytes if png_bytes is not None else make_png(width, height)
        self.commands: list[list[str]] = []
        self.fail_on_cmd_substring: str | None = None

    def probe(self) -> wayland.SessionState:
        env = dict(self._base_env)
        if self.desktop_ready:
            env["WAYLAND_DISPLAY"] = "wayland-1"
            env["SWAYSOCK"] = "/run/user/1000/sway-ipc.1000.42.sock"
        return wayland.SessionState(
            env=env,
            desktop_ready=self.desktop_ready,
            display=wayland.DisplayInfo(self.width, self.height, self.scale)
            if self.desktop_ready
            else wayland.DisplayInfo(0, 0, 1.0),
            has_swaymsg=self.has_swaymsg,
            has_grim=self.has_grim,
            has_wtype=self.has_wtype,
        )

    def _run_tool(
        self, argv: list[str], env: dict[str, str], timeout: float = wayland.TOOL_TIMEOUT_S
    ) -> subprocess.CompletedProcess[bytes]:
        self.commands.append(argv)
        joined = " ".join(argv)
        if self.fail_on_cmd_substring and self.fail_on_cmd_substring in joined:
            return subprocess.CompletedProcess(argv, 1, b"", b"simulated failure")
        if argv[0] == "grim":
            return subprocess.CompletedProcess(argv, 0, self.png_bytes, b"")
        return subprocess.CompletedProcess(argv, 0, b"[]", b"")


class AgentProtocolTests(unittest.TestCase):
    def test_peer_cid_allows_only_host(self) -> None:
        self.assertTrue(agent.is_host_peer((2, 50000)))
        self.assertFalse(agent.is_host_peer((3, 50000)))
        self.assertFalse(agent.is_host_peer((1, 50000)))
        self.assertFalse(agent.is_host_peer("not-a-tuple"))

    def test_hello_reports_capabilities_and_geometry(self) -> None:
        backend = StubWaylandBackend(width=1440, height=900, scale=2.0)
        reply = exchange(agent.AgentServer(backend), {"op": "hello"})
        self.assertTrue(reply["ok"])
        self.assertEqual(reply["version"], 1)
        self.assertEqual((reply["width"], reply["height"], reply["scale"]), (1440, 900, 2.0))
        self.assertEqual(
            reply["capabilities"],
            {
                "exec": True,
                "screenshot": True,
                "pointer": True,
                "keyboard": True,
                "text": True,
                "desktop_ready": True,
            },
        )

    def test_hello_and_desktop_ops_report_not_ready_before_session_starts(self) -> None:
        backend = StubWaylandBackend(desktop_ready=False)
        server = agent.AgentServer(backend)
        hello = exchange(server, {"op": "hello"})
        self.assertTrue(hello["ok"])
        self.assertFalse(hello["capabilities"]["desktop_ready"])
        self.assertTrue(hello["capabilities"]["exec"])
        self.assertFalse(hello["capabilities"]["screenshot"])

        for req in [
            {"op": "screenshot"},
            {"op": "click", "x": 10, "y": 10, "button": "left", "count": 1},
            {"op": "type", "text": "hi"},
            {"op": "key", "key": "return", "modifiers": []},
        ]:
            reply = exchange(server, req)
            self.assertFalse(reply["ok"])
            self.assertEqual(reply["error"], "not_ready")

    def test_missing_tools_report_unsupported_capability(self) -> None:
        backend = StubWaylandBackend(has_grim=False, has_wtype=False)
        server = agent.AgentServer(backend)
        hello = exchange(server, {"op": "hello"})
        self.assertTrue(hello["capabilities"]["desktop_ready"])
        self.assertFalse(hello["capabilities"]["screenshot"])
        self.assertFalse(hello["capabilities"]["keyboard"])
        self.assertFalse(hello["capabilities"]["text"])

        shot = exchange(server, {"op": "screenshot"})
        self.assertFalse(shot["ok"])
        self.assertEqual(shot["error"], "unsupported")

        typed = exchange(server, {"op": "type", "text": "hello"})
        self.assertFalse(typed["ok"])
        self.assertEqual(typed["error"], "unsupported")

    def test_screenshot_validates_png_header_and_dimensions(self) -> None:
        backend = StubWaylandBackend(width=1280, height=800)
        reply = exchange(agent.AgentServer(backend), {"op": "screenshot"})
        self.assertTrue(reply["ok"])
        self.assertEqual((reply["width"], reply["height"]), (1280, 800))
        decoded = base64.b64decode(reply["png"])
        self.assertEqual(wayland.png_dimensions(decoded), (1280, 800))

        # Corrupt / non-PNG output from grim is rejected.
        bad_backend = StubWaylandBackend(png_bytes=b"not a png at all")
        bad_reply = exchange(agent.AgentServer(bad_backend), {"op": "screenshot"})
        self.assertFalse(bad_reply["ok"])
        self.assertEqual(bad_reply["error"], "failed")

        # Zero-sized PNG is rejected.
        zero_backend = StubWaylandBackend(png_bytes=make_png(0, 800))
        zero_reply = exchange(agent.AgentServer(zero_backend), {"op": "screenshot"})
        self.assertFalse(zero_reply["ok"])
        self.assertEqual(zero_reply["error"], "failed")

    def test_click_scales_pixel_coordinates_and_rejects_out_of_bounds(self) -> None:
        backend = StubWaylandBackend(width=2560, height=1600, scale=2.0)
        server = agent.AgentServer(backend)
        reply = exchange(server, {"op": "click", "x": 640, "y": 400, "button": "right", "count": 2})
        self.assertTrue(reply["ok"])
        sway_cmds = [cmd[-1] for cmd in backend.commands]
        self.assertEqual(
            sway_cmds,
            [
                "seat - cursor set 320.00 200.00",
                "seat - cursor press BTN_RIGHT",
                "seat - cursor release BTN_RIGHT",
                "seat - cursor press BTN_RIGHT",
                "seat - cursor release BTN_RIGHT",
            ],
        )

        # Out-of-bounds pixel coordinate is rejected cleanly.
        oob = exchange(server, {"op": "click", "x": 2560, "y": 100, "button": "left", "count": 1})
        self.assertFalse(oob["ok"])
        self.assertEqual(oob["error"], "invalid")

    def test_type_unicode_and_multiline_and_key_modifiers_release_cleanly(self) -> None:
        backend = StubWaylandBackend()
        server = agent.AgentServer(backend)
        reply = exchange(server, {"op": "type", "text": "Hello café 日本語 🚀\nline2\tend"})
        self.assertTrue(reply["ok"])
        self.assertEqual(
            backend.commands,
            [
                ["wtype", "-d", "8", "--", "Hello café 日本語 🚀"],
                ["wtype", "-d", "8", "-k", "Return"],
                ["wtype", "-d", "8", "--", "line2"],
                ["wtype", "-d", "8", "-k", "Tab"],
                ["wtype", "-d", "8", "--", "end"],
            ],
        )

        backend.commands.clear()
        key_reply = exchange(
            server, {"op": "key", "key": "a", "modifiers": ["control", "shift"]}
        )
        self.assertTrue(key_reply["ok"])
        self.assertEqual(
            backend.commands,
            [["wtype", "-d", "8", "-M", "ctrl", "-M", "shift", "-k", "a", "-m", "shift", "-m", "ctrl"]],
        )

        # `fn` modifier fails clearly as unsupported.
        fn_reply = exchange(server, {"op": "key", "key": "f1", "modifiers": ["fn"]})
        self.assertFalse(fn_reply["ok"])
        self.assertEqual(fn_reply["error"], "unsupported")

        # Unknown key fails as invalid.
        bad_key = exchange(server, {"op": "key", "key": "not_a_key", "modifiers": []})
        self.assertFalse(bad_key["ok"])
        self.assertEqual(bad_key["error"], "invalid")

    def test_exec_runs_commands_handles_timeout_and_truncates_output(self) -> None:
        backend = StubWaylandBackend(desktop_ready=False)
        server = agent.AgentServer(backend)
        with tempfile.TemporaryDirectory() as tmp:
            ok = exchange(
                server,
                {
                    "op": "exec",
                    "argv": [sys.executable, "-c", "import os; print(os.getcwd())"],
                    "cwd": tmp,
                    "timeout_ms": 5000,
                },
            )
            self.assertTrue(ok["ok"])
            self.assertEqual(ok["status"], 0)
            self.assertEqual(os.path.realpath(ok["stdout"].strip()), os.path.realpath(tmp))
            self.assertFalse(ok["timed_out"])

            # Timeout kills entire process group cleanly, including a child
            # that ignores SIGTERM and redirects output after the leader exits.
            pid_file = os.path.join(tmp, "child.pid")
            script = (
                "import os, signal, subprocess, sys, time; "
                "child = subprocess.Popen([sys.executable, '-c', "
                "'import signal, time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(30)'], "
                "stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL); "
                f"open({pid_file!r}, 'w').write(str(child.pid)); "
                "time.sleep(30)"
            )
            slow = exchange(
                server,
                {
                    "op": "exec",
                    "argv": [sys.executable, "-c", script],
                    "timeout_ms": 300,
                },
            )
            self.assertTrue(slow["ok"])
            self.assertTrue(slow["timed_out"])
            child_pid = int(open(pid_file).read().strip())
            # Wait briefly for kernel process reaping after SIGKILL.
            for _ in range(20):
                try:
                    os.kill(child_pid, 0)
                except ProcessLookupError:
                    break
                time.sleep(0.05)
            with self.assertRaises(ProcessLookupError):
                os.kill(child_pid, 0)

    def test_exec_output_drain_shares_single_deadline(self) -> None:
        backend = StubWaylandBackend(desktop_ready=False)
        server = agent.AgentServer(backend)
        # Parent exits immediately while child closes stdout after 0.35s and
        # holds stderr open for 30s; shared 500ms timeout must kill stderr
        # promptly rather than granting a second full 500ms window.
        script = (
            "import subprocess, sys; "
            "subprocess.Popen([sys.executable, '-c', "
            "'import sys, time; time.sleep(0.35); sys.stdout.close(); time.sleep(30)'])"
        )
        start = time.monotonic()
        reply = exchange(
            server,
            {
                "op": "exec",
                "argv": [sys.executable, "-c", script],
                "timeout_ms": 500,
            },
        )
        elapsed = time.monotonic() - start
        self.assertTrue(reply["ok"])
        self.assertLess(elapsed, 0.85)

    def test_session_recovery_after_compositor_restart_picks_newest_sockets(self) -> None:
        with tempfile.TemporaryDirectory() as runtime_dir:
            old_wl = os.path.join(runtime_dir, "wayland-0")
            new_wl = os.path.join(runtime_dir, "wayland-1")
            x11_sock = os.path.join(runtime_dir, "X0")
            s1 = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            s2 = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sx = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            try:
                s1.bind(old_wl)
                time.sleep(0.02)
                s2.bind(new_wl)
                sx.bind(x11_sock)
                backend = wayland.WaylandBackend(
                    base_env={
                        "XDG_RUNTIME_DIR": runtime_dir,
                        # Stale WAYLAND_DISPLAY and DISPLAY from a dead compositor:
                        "WAYLAND_DISPLAY": "wayland-stale",
                        "DISPLAY": ":99",
                    }
                )
                discovered = backend._discover_wayland_display(runtime_dir)
                self.assertEqual(discovered, "wayland-1")
                discovered_x11 = backend._discover_x11_display(runtime_dir)
                self.assertEqual(discovered_x11, ":0")
            finally:
                s1.close()
                s2.close()
                sx.close()

    def test_parse_sway_outputs_handles_focused_output_and_scale(self) -> None:
        payload = json.dumps(
            [
                {"name": "Virtual-2", "active": False, "focused": False, "scale": 1.0, "current_mode": {"width": 800, "height": 600}},
                {"name": "Virtual-1", "active": True, "focused": True, "scale": 1.5, "current_mode": {"width": 1920, "height": 1200}},
            ]
        ).encode("utf-8")
        info = wayland.parse_sway_outputs(payload)
        self.assertEqual(info, wayland.DisplayInfo(width=1920, height=1200, scale=1.5))

    def test_stopping_server_refuses_new_requests_cleanly(self) -> None:
        server = agent.AgentServer(StubWaylandBackend())
        server.request_stop()
        reply = exchange(server, {"op": "hello"})
        self.assertFalse(reply["ok"])
        self.assertEqual(reply["error"], "failed")


if __name__ == "__main__":
    unittest.main()
