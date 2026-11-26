"""Wayland/Sway desktop session discovery, screen capture, and input for Sheprd Agent.

Runs inside the logged-in graphical user session and discovers live Wayland and
Sway IPC sockets dynamically so compositor restarts recover automatically
without leaving stale environment variables behind.
"""

from __future__ import annotations

import base64
import glob
import json
import math
import os
import shutil
import stat
import struct
import subprocess
import time
from dataclasses import dataclass
from typing import Any

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
MAX_PNG_DIMENSION = 16_384
MAX_REPLY_BYTES = 64 << 20
MAX_TYPE_CHARS = 4096
DEFAULT_TYPE_DELAY_MS = 8
CLICK_PRESS_GAP_S = 0.015
CLICK_REPEAT_GAP_S = 0.020
TOOL_TIMEOUT_S = 15.0

BUTTON_MAP: dict[str, str] = {
    "left": "BTN_LEFT",
    "right": "BTN_RIGHT",
    "middle": "BTN_MIDDLE",
}

MODIFIER_MAP: dict[str, str] = {
    "control": "ctrl",
    "ctrl": "ctrl",
    "shift": "shift",
    "option": "alt",
    "alt": "alt",
    "cmd": "logo",
    "super": "logo",
    "meta": "logo",
}

# Portable Sheprd key names (aligned with macos/sheprd-machine-agent/Input.swift)
# mapped to XKB keysym names accepted by `wtype -k`.
KEY_MAP: dict[str, str] = {
    **{c: c for c in "abcdefghijklmnopqrstuvwxyz0123456789"},
    "-": "minus",
    "=": "equal",
    "[": "bracketleft",
    "]": "bracketright",
    "\\": "backslash",
    ";": "semicolon",
    "'": "apostrophe",
    "`": "grave",
    ",": "comma",
    ".": "period",
    "/": "slash",
    "return": "Return",
    "enter": "Return",
    "tab": "Tab",
    "space": "space",
    "delete": "BackSpace",
    "backspace": "BackSpace",
    "forwarddelete": "Delete",
    "escape": "Escape",
    "esc": "Escape",
    "home": "Home",
    "end": "End",
    "pageup": "Page_Up",
    "pagedown": "Page_Down",
    "left": "Left",
    "right": "Right",
    "up": "Up",
    "down": "Down",
    **{f"f{n}": f"F{n}" for n in range(1, 21)},
}


class Refusal(Exception):
    """Structured refusal matching the Sheprd Agent wire contract."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message

    def reply(self) -> dict[str, Any]:
        return {"ok": False, "error": self.code, "message": self.message}


@dataclass(frozen=True)
class DisplayInfo:
    width: int
    height: int
    scale: float


@dataclass(frozen=True)
class SessionState:
    env: dict[str, str]
    desktop_ready: bool
    display: DisplayInfo
    has_swaymsg: bool
    has_grim: bool
    has_wtype: bool

    def capabilities(self) -> dict[str, bool]:
        return {
            "exec": True,
            "screenshot": self.desktop_ready and self.has_grim,
            "pointer": self.desktop_ready and self.has_swaymsg,
            "keyboard": self.desktop_ready and self.has_wtype,
            "text": self.desktop_ready and self.has_wtype,
            "desktop_ready": self.desktop_ready,
        }


def is_socket(path: str) -> bool:
    try:
        return stat.S_ISSOCK(os.stat(path).st_mode)
    except OSError:
        return False


def png_dimensions(data: bytes) -> tuple[int, int] | None:
    """Validates PNG signature and IHDR header, returning `(width, height)`."""
    if len(data) < 24:
        return None
    if data[:8] != PNG_SIGNATURE or data[12:16] != b"IHDR":
        return None
    width, height = struct.unpack(">II", data[16:24])
    if width == 0 or height == 0 or width > MAX_PNG_DIMENSION or height > MAX_PNG_DIMENSION:
        return None
    return width, height


def parse_sway_outputs(raw_json: bytes) -> DisplayInfo | None:
    """Extracts pixel dimensions and scale from `swaymsg -t get_outputs` JSON."""
    try:
        outputs = json.loads(raw_json.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError):
        return None
    if not isinstance(outputs, list):
        return None
    active = [o for o in outputs if isinstance(o, dict) and o.get("active")]
    if not active:
        return None
    chosen = next((o for o in active if o.get("focused")), active[0])
    scale_raw = chosen.get("scale", 1.0)
    scale = float(scale_raw) if isinstance(scale_raw, (int, float)) and scale_raw > 0 else 1.0
    if not math.isfinite(scale) or scale <= 0:
        scale = 1.0
    mode = chosen.get("current_mode")
    rect = chosen.get("rect")
    width = 0
    height = 0
    if isinstance(mode, dict) and isinstance(mode.get("width"), int) and isinstance(mode.get("height"), int):
        width = mode["width"]
        height = mode["height"]
    elif isinstance(rect, dict) and isinstance(rect.get("width"), int) and isinstance(rect.get("height"), int):
        width = round(rect["width"] * scale)
        height = round(rect["height"] * scale)
    if width <= 0 or height <= 0:
        return None
    return DisplayInfo(width=width, height=height, scale=scale)


def build_key_args(key_name: str, modifiers: list[str]) -> list[str]:
    """Builds `wtype` arguments pressing modifiers, tapping key, and releasing modifiers."""
    if not isinstance(key_name, str) or not key_name:
        raise Refusal("invalid", "key needs a non-empty key name")
    keysym = KEY_MAP.get(key_name.lower())
    if keysym is None:
        raise Refusal("invalid", f"unknown key {key_name}")
    mapped_mods: list[str] = []
    for raw_mod in modifiers:
        if not isinstance(raw_mod, str):
            raise Refusal("invalid", "modifiers must be strings")
        lower = raw_mod.lower()
        if lower == "fn":
            raise Refusal("unsupported", "fn modifier is not supported on Linux")
        mapped = MODIFIER_MAP.get(lower)
        if mapped is None:
            raise Refusal("invalid", f"unknown modifier {raw_mod}")
        if mapped not in mapped_mods:
            mapped_mods.append(mapped)

    args = ["wtype", "-d", str(DEFAULT_TYPE_DELAY_MS)]
    for mod in mapped_mods:
        args.extend(["-M", mod])
    args.extend(["-k", keysym])
    for mod in reversed(mapped_mods):
        args.extend(["-m", mod])
    return args


def build_type_commands(text: str) -> list[list[str]]:
    """Splits text into `wtype` invocations preserving newlines and tabs."""
    if not isinstance(text, str):
        raise Refusal("invalid", "type needs text")
    if len(text) > MAX_TYPE_CHARS:
        raise Refusal("invalid", f"type takes at most {MAX_TYPE_CHARS} characters at once")
    if not text:
        return []
    commands: list[list[str]] = []
    buf: list[str] = []

    def flush() -> None:
        if buf:
            commands.append(["wtype", "-d", str(DEFAULT_TYPE_DELAY_MS), "--", "".join(buf)])
            buf.clear()

    for ch in text:
        if ch == "\n":
            flush()
            commands.append(["wtype", "-d", str(DEFAULT_TYPE_DELAY_MS), "-k", "Return"])
        elif ch == "\r":
            continue
        elif ch == "\t":
            flush()
            commands.append(["wtype", "-d", str(DEFAULT_TYPE_DELAY_MS), "-k", "Tab"])
        else:
            buf.append(ch)
    flush()
    return commands


class WaylandBackend:
    """Manages Wayland/Sway session lookup, capture, and input."""

    def __init__(self, base_env: dict[str, str] | None = None) -> None:
        self._base_env = dict(base_env) if base_env is not None else dict(os.environ)

    def _runtime_dir(self) -> str:
        runtime = self._base_env.get("XDG_RUNTIME_DIR")
        if runtime:
            return runtime
        return f"/run/user/{os.getuid()}"

    def _discover_wayland_display(self, runtime_dir: str) -> str | None:
        current = self._base_env.get("WAYLAND_DISPLAY")
        if current:
            candidate = current if os.path.isabs(current) else os.path.join(runtime_dir, current)
            if is_socket(candidate):
                return current
        sockets = sorted(
            (p for p in glob.glob(os.path.join(runtime_dir, "wayland-*")) if not p.endswith(".lock") and is_socket(p)),
            key=lambda p: os.path.getmtime(p),
            reverse=True,
        )
        return os.path.basename(sockets[0]) if sockets else None

    def _discover_x11_display(self, x11_dir: str = "/tmp/.X11-unix") -> str | None:
        current = self._base_env.get("DISPLAY")
        if current and current.startswith(":"):
            num = current[1:].split(".", 1)[0]
            if num.isdigit() and is_socket(os.path.join(x11_dir, f"X{num}")):
                return current
        sockets = sorted(
            (p for p in glob.glob(os.path.join(x11_dir, "X*")) if is_socket(p)),
            key=lambda p: os.path.getmtime(p),
            reverse=True,
        )
        for path in sockets:
            base = os.path.basename(path)
            if base.startswith("X") and base[1:].isdigit():
                return f":{base[1:]}"
        return None

    def _discover_sway_sockets(self, runtime_dir: str) -> list[str]:
        candidates: list[str] = []
        current = self._base_env.get("SWAYSOCK")
        if current and is_socket(current):
            candidates.append(current)
        uid = os.getuid()
        found = sorted(
            (p for p in glob.glob(os.path.join(runtime_dir, f"sway-ipc.{uid}.*.sock")) if is_socket(p)),
            key=lambda p: os.path.getmtime(p),
            reverse=True,
        )
        for sock in found:
            if sock not in candidates:
                candidates.append(sock)
        return candidates

    def _run_tool(self, argv: list[str], env: dict[str, str], timeout: float = TOOL_TIMEOUT_S) -> subprocess.CompletedProcess[bytes]:
        try:
            return subprocess.run(
                argv,
                env=env,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=timeout,
                check=False,
            )
        except FileNotFoundError as exc:
            raise Refusal("unsupported", f"required Wayland tool is not installed: {argv[0]}") from exc
        except subprocess.TimeoutExpired as exc:
            raise Refusal("failed", f"{argv[0]} timed out") from exc
        except OSError as exc:
            raise Refusal("failed", f"cannot run {argv[0]}: {exc}") from exc

    def _which(self, tool: str, env: dict[str, str]) -> bool:
        return shutil.which(tool, path=env.get("PATH")) is not None

    def probe(self) -> SessionState:
        """Discovers the live Wayland/Sway session and computes capabilities."""
        runtime_dir = self._runtime_dir()
        env = dict(self._base_env)
        env["XDG_RUNTIME_DIR"] = runtime_dir
        env.setdefault("PATH", "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin")
        has_swaymsg = self._which("swaymsg", env)
        has_grim = self._which("grim", env)
        has_wtype = self._which("wtype", env)

        wayland = self._discover_wayland_display(runtime_dir)
        if wayland:
            env["WAYLAND_DISPLAY"] = wayland
        else:
            env.pop("WAYLAND_DISPLAY", None)

        x11_display = self._discover_x11_display()
        if x11_display:
            env["DISPLAY"] = x11_display
        else:
            env.pop("DISPLAY", None)

        display: DisplayInfo | None = None
        if wayland and has_swaymsg:
            for sock in self._discover_sway_sockets(runtime_dir):
                probe_env = dict(env)
                probe_env["SWAYSOCK"] = sock
                try:
                    res = self._run_tool(["swaymsg", "-s", sock, "-t", "get_outputs"], probe_env, timeout=5.0)
                except Refusal:
                    continue
                if res.returncode == 0:
                    parsed = parse_sway_outputs(res.stdout)
                    if parsed is not None:
                        env["SWAYSOCK"] = sock
                        display = parsed
                        break

        desktop_ready = display is not None
        return SessionState(
            env=env,
            desktop_ready=desktop_ready,
            display=display or DisplayInfo(width=0, height=0, scale=1.0),
            has_swaymsg=has_swaymsg,
            has_grim=has_grim,
            has_wtype=has_wtype,
        )

    def session_env(self) -> dict[str, str]:
        return self.probe().env

    def hello(self, version: int) -> dict[str, Any]:
        state = self.probe()
        caps = state.capabilities()
        return {
            "version": version,
            "width": state.display.width,
            "height": state.display.height,
            "scale": state.display.scale,
            # Legacy fields stay honest while callers migrate to `capabilities`.
            "accessibility": caps["pointer"] and caps["keyboard"] and caps["text"],
            "screen_recording": caps["screenshot"],
            "capabilities": caps,
        }

    def screenshot(self) -> dict[str, Any]:
        state = self.probe()
        if not state.desktop_ready:
            raise Refusal("not_ready", "Wayland desktop session is not ready yet")
        if not state.has_grim:
            raise Refusal("unsupported", "screen capture is unavailable because grim is not installed")
        res = self._run_tool(["grim", "-t", "png", "-"], state.env)
        if res.returncode != 0:
            detail = res.stderr.decode("utf-8", errors="replace").strip() or "grim failed"
            raise Refusal("failed", f"screenshot failed: {detail}")
        dims = png_dimensions(res.stdout)
        if dims is None:
            raise Refusal("failed", "grim did not produce a valid PNG image")
        encoded = base64.b64encode(res.stdout).decode("ascii")
        if len(encoded) > MAX_REPLY_BYTES - 1024:
            raise Refusal("failed", "screenshot exceeds maximum reply size")
        return {"png": encoded, "width": dims[0], "height": dims[1]}

    def click(self, request: dict[str, Any]) -> dict[str, Any]:
        raw_x = request.get("x")
        raw_y = request.get("y")
        if isinstance(raw_x, bool) or isinstance(raw_y, bool) or not isinstance(raw_x, (int, float)) or not isinstance(raw_y, (int, float)):
            raise Refusal("invalid", "click needs numeric x and y")
        x = float(raw_x)
        y = float(raw_y)
        if not math.isfinite(x) or not math.isfinite(y) or x < 0 or y < 0:
            raise Refusal("invalid", "click coordinates must be non-negative finite numbers")
        raw_button = request.get("button", "left")
        if not isinstance(raw_button, str) or raw_button not in BUTTON_MAP:
            raise Refusal("invalid", f"unknown button {raw_button}")
        raw_count = request.get("count", 1)
        if isinstance(raw_count, bool) or not isinstance(raw_count, int) or not (1 <= raw_count <= 3):
            raise Refusal("invalid", "click count must be 1, 2, or 3")

        state = self.probe()
        if not state.desktop_ready:
            raise Refusal("not_ready", "Wayland desktop session is not ready yet")
        if not state.has_swaymsg:
            raise Refusal("unsupported", "pointer input is unavailable because swaymsg is not installed")
        if x >= state.display.width or y >= state.display.height:
            raise Refusal(
                "invalid",
                f"click ({x:g}, {y:g}) is outside display bounds {state.display.width}x{state.display.height}",
            )

        logical_x = x / state.display.scale
        logical_y = y / state.display.scale
        button = BUTTON_MAP[raw_button]
        sock = state.env["SWAYSOCK"]

        self._sway_cmd(sock, state.env, f"seat - cursor set {logical_x:.2f} {logical_y:.2f}")
        for idx in range(raw_count):
            pressed = False
            try:
                self._sway_cmd(sock, state.env, f"seat - cursor press {button}")
                pressed = True
                time.sleep(CLICK_PRESS_GAP_S)
            finally:
                if pressed:
                    self._sway_cmd(sock, state.env, f"seat - cursor release {button}")
            if idx + 1 < raw_count:
                time.sleep(CLICK_REPEAT_GAP_S)
        return {}

    def _sway_cmd(self, sock: str, env: dict[str, str], command: str) -> None:
        res = self._run_tool(["swaymsg", "-s", sock, "--", command], env)
        if res.returncode != 0:
            detail = res.stderr.decode("utf-8", errors="replace").strip() or "swaymsg failed"
            raise Refusal("failed", f"pointer action failed: {detail}")

    def type_text(self, request: dict[str, Any]) -> dict[str, Any]:
        text = request.get("text")
        commands = build_type_commands(text)  # validates type/length before probing
        state = self.probe()
        if not state.desktop_ready:
            raise Refusal("not_ready", "Wayland desktop session is not ready yet")
        if not state.has_wtype:
            raise Refusal("unsupported", "text input is unavailable because wtype is not installed")
        for argv in commands:
            res = self._run_tool(argv, state.env, timeout=45.0)
            if res.returncode != 0:
                detail = res.stderr.decode("utf-8", errors="replace").strip() or "wtype failed"
                raise Refusal("failed", f"text input failed: {detail}")
        return {}

    def key(self, request: dict[str, Any]) -> dict[str, Any]:
        key_name = request.get("key")
        modifiers = request.get("modifiers", [])
        if not isinstance(modifiers, list):
            raise Refusal("invalid", "modifiers must be a list")
        argv = build_key_args(key_name, modifiers)
        state = self.probe()
        if not state.desktop_ready:
            raise Refusal("not_ready", "Wayland desktop session is not ready yet")
        if not state.has_wtype:
            raise Refusal("unsupported", "keyboard input is unavailable because wtype is not installed")
        res = self._run_tool(argv, state.env)
        if res.returncode != 0:
            detail = res.stderr.decode("utf-8", errors="replace").strip() or "wtype failed"
            raise Refusal("failed", f"key input failed: {detail}")
        return {}
