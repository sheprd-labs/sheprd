#!/usr/bin/env python3
"""Benchmark native terminal output and scrolling in both window modes.

See docs/moonframe.md for the isolated VM setup and measurement contract.
"""

import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import threading
import time

from bench_frames import Sandbox, FRAME, UNITS, cpu_seconds, percentile, stop


def measurements(samples, start, end, cpu_start, cpu_end):
    seconds = end - start
    values = [duration for _, duration in samples]
    gaps = [(right[0] - left[0]) * 1000 for left, right in zip(samples, samples[1:])]
    first = (samples[0][0] - start) * 1000 if samples else seconds * 1000
    last = (end - samples[-1][0]) * 1000 if samples else seconds * 1000
    return {
        "frames": len(values), "seconds": seconds,
        "fps": len(values) / seconds,
        "p50_ms": percentile(values, 0.50) if values else None,
        "p95_ms": percentile(values, 0.95) if values else None,
        "p99_ms": percentile(values, 0.99) if values else None,
        "max_ms": max(values) if values else None,
        "over_16_7_pct": 100 * sum(value > 16.7 for value in values) / len(values) if values else None,
        "gap_p50_ms": percentile(gaps, 0.50), "gap_p95_ms": percentile(gaps, 0.95),
        "cpu_pct": 100 * (cpu_end - cpu_start) / seconds,
        "first_frame_delay_ms": first, "last_frame_gap_ms": last,
        "max_gap_ms": max(gaps, default=0),
        "max_silence_ms": max([*gaps, first, last]),
    }


def pixel_measurements(records, start, end):
    captured = [record["time"] for record in records if "time" in record]
    starts = [record["start"] for record in records if "start" in record]
    ends = [record["end"] for record in records if "end" in record]
    if not captured or not starts or not ends or starts[0] > start or ends[-1] < end:
        raise RuntimeError("pixel probe did not cover the measured interval")
    changes = [stamp for stamp in captured if start <= stamp < end]
    boundaries = [start, *changes, end]
    return {
        "pixel_changes": len(changes),
        "pixel_change_hz": len(changes) / (end - start),
        "pixel_max_silence_ms": max(right - left for left, right in zip(boundaries, boundaries[1:])) * 1000,
    }


def sample_scroll_positions(deadline, read_position, now=time.monotonic, sleep=time.sleep):
    samples = []
    while now() < deadline:
        position = read_position()
        stamp = now()
        if stamp >= deadline:
            break
        samples.append({"time": stamp, "offset": position["pane"]["scroll"]["offset_from_bottom"]})
        sleep(min(0.5, max(0, deadline - now())))
    return samples


def validate_scroll_positions(samples, start, end):
    offsets = {sample["offset"] for sample in samples if start <= sample["time"] < end}
    if len(offsets) < 2:
        raise RuntimeError("scroll position did not change during measurement")


def finish_helper(process, name):
    if process.poll() is not None:
        raise RuntimeError(f"{name} exited before measurement ended")
    process.stdin.close()
    if process.wait(timeout=5):
        raise RuntimeError(f"{name} failed")


class CheckedSandbox(Sandbox):
    def _populate(self, home, trees):
        self.env["PATH"] = os.environ.get("PATH", "/usr/bin:/bin")
        try:
            return super()._populate(home, trees)
        except BaseException:
            self.close()
            raise

    def run(self, *args):
        result = subprocess.run([self.herdr, *args], env=self.env, capture_output=True,
                                text=True, timeout=15)
        if result.returncode:
            raise RuntimeError(f"herdr {args}: {result.stdout} {result.stderr}")
        try:
            parsed = json.loads(result.stdout)
        except json.JSONDecodeError:
            return {"text": result.stdout}
        if parsed.get("error"):
            raise RuntimeError(f"herdr {args}: {parsed}")
        return parsed.get("result", parsed)


def run_case(args, mode, round_number):
    destination = Path(args.results) / f"{mode}-{round_number}"
    destination.mkdir(parents=True, exist_ok=True)
    sandbox = CheckedSandbox(args.herdr, args.trees)
    app = None
    scroll_process = None
    probe = None
    pixel_log = None
    awake = subprocess.Popen(["/usr/bin/caffeinate", "-disu", "-t", "3600"])
    frames = []
    try:
        workspaces = sandbox.workspaces()
        sandbox.run("workspace", "focus", workspaces[0])
        current = sandbox.run("pane", "current")
        (destination / "pane-current.json").write_text(json.dumps(current))
        pane = current.get("pane", {}).get("pane_id") or current.get("pane_id") or sandbox.panes[0]
        environment = {**sandbox.app_env(), "ZED_MEASUREMENTS": "1"}
        if mode == "moonframe":
            environment["SHEPRD_MOONFRAME"] = "1"
        command = [args.app] + (["--classic"] if mode == "classic" else [])
        log = (destination / "app.log").open("w")
        app = subprocess.Popen(command, env=environment, stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, text=True, errors="replace")

        def read_frames():
            for line in app.stdout:
                log.write(line)
                match = FRAME.search(line)
                if match:
                    frames.append((time.monotonic(), float(match[1]) * UNITS[match[2]]))
            log.flush()

        reader = threading.Thread(target=read_frames, daemon=True)
        reader.start()
        time.sleep(10)
        if app.poll() is not None:
            raise RuntimeError(f"app exited: {app.returncode}")
        resized = subprocess.run([args.app, "window", "size", "1000x650", "--json"],
                                 env=environment, capture_output=True, text=True, timeout=15)
        if resized.returncode:
            raise RuntimeError(f"window resize: {resized.stdout} {resized.stderr}")
        listed = subprocess.run([args.app, "window", "list", "--json"],
                                env=environment, check=True, capture_output=True, text=True)
        window_id = str(json.loads(listed.stdout)[0]["id"])
        subprocess.run([args.app, "window", "focus", window_id, "--json"],
                       env=environment, check=True, capture_output=True)
        time.sleep(2)
        subprocess.run(["/usr/sbin/screencapture", "-x", str(destination / "screen.png")], check=True)
        status = subprocess.run([args.app, "status", "--json"], env=environment,
                                capture_output=True, text=True, timeout=15)
        (destination / "status.json").write_text(status.stdout + status.stderr)
        if not json.loads(status.stdout)["windows"][0]["active"]:
            raise RuntimeError("benchmark window is not active")
        rows = {}
        phases = [("idle", 0), ("steady", 1), ("flood", 100)]
        if args.scroll:
            phases.append(("scroll", 0))
        if args.scroll_only:
            phases = [("scroll", 0)]
        if args.steady_only:
            phases = [("steady", 1)]
        if args.output_only:
            phases = [("steady", 1), ("flood", 100)]
        for phase, batch in phases:
            scroll_process = None
            if phase == "scroll":
                sandbox.run("pane", "run", pane, "awk 'BEGIN { for (line=1; line<=3000; line++) print \"SCROLL LINE \" line \" terminal history 0123456789 abcdefghijklmnopqrstuvwxyz\" }'")
                time.sleep(3)
                if args.pixels:
                    pixel_log = (destination / "pixels.jsonl").open("w")
                    probe = subprocess.Popen(
                        [args.pixels, "--until-eof", str(destination / "first-frame.bgra")],
                        stdin=subprocess.PIPE, stdout=pixel_log, stderr=pixel_log,
                    )
                    time.sleep(0.5)
                scroll_process = subprocess.Popen([args.scroll, "--until-eof"], stdin=subprocess.PIPE)
            if batch:
                generator = sandbox.root / "output.py"
                generator.write_text(
                    "import sys,time\n"
                    "for sequence in range(100000):\n"
                    f" sys.stdout.write(''.join(f'BENCH {{sequence:08d}} {{line:03d}} compile module terminal output 0123456789 abcdefghijklmnopqrstuvwxyz\\n' for line in range({batch})))\n"
                    " sys.stdout.flush()\n"
                    " time.sleep(0.01)\n"
                )
                result = sandbox.run("pane", "run", pane,
                                     f"{shlex.quote(args.python)} {shlex.quote(str(generator))}")
                (destination / f"{phase}-command.json").write_text(json.dumps(result))
            time.sleep(2)
            if phase == "scroll":
                warmup_position = sandbox.run("pane", "current")
                (destination / "scroll-warmup-position.json").write_text(json.dumps(warmup_position))
            cpu_start = cpu_seconds(app.pid)
            start = time.monotonic()
            positions = []
            if phase == "scroll":
                positions = sample_scroll_positions(
                    start + args.seconds, lambda: sandbox.run("pane", "current"),
                )
            else:
                time.sleep(args.seconds)
            cpu_end = cpu_seconds(app.pid)
            end = time.monotonic()
            if scroll_process is not None:
                finish_helper(scroll_process, "native scroll helper")
            if phase == "scroll" and probe is not None:
                finish_helper(probe, "pixel probe")
                pixel_log.close()
            samples = [(stamp, duration) for stamp, duration in frames if start <= stamp < end]
            (destination / f"{phase}-pane.json").write_text(json.dumps(sandbox.run("pane", "read", pane)))
            (destination / f"{phase}-position.json").write_text(json.dumps(sandbox.run("pane", "current")))
            if phase == "scroll":
                current = sandbox.run("pane", "current")
                (destination / "scroll-position.json").write_text(json.dumps(current))
                (destination / "scroll-samples.json").write_text(json.dumps(positions))
                validate_scroll_positions(positions, start, end)
            subprocess.run(["/usr/sbin/screencapture", "-x", str(destination / f"{phase}.png")], check=True)
            if not frames:
                raise RuntimeError(f"no frame measurements for {mode} {phase}")
            rows[phase] = measurements(samples, start, end, cpu_start, cpu_end)
            rows[phase].update(measurement_start=start, measurement_end=end)
            if phase == "scroll" and args.pixels:
                records = [json.loads(line) for line in (destination / "pixels.jsonl").read_text().splitlines()]
                rows[phase].update(pixel_measurements(records, start, end))
            (destination / f"{phase}-frames.json").write_text(json.dumps(samples))
            if batch:
                readback = sandbox.run("pane", "read", pane)
                (destination / f"{phase}-pane.json").write_text(json.dumps(readback))
                if "BENCH" not in json.dumps(readback):
                    raise RuntimeError(f"terminal output missing for {mode} {phase}")
                if round_number == 1:
                    subprocess.run(["/usr/sbin/screencapture", "-x", str(destination / f"{phase}.png")], check=True)
                sandbox.run("pane", "send-keys", pane, "C-c")
                time.sleep(1)
            print(json.dumps({"mode": mode, "round": round_number, "phase": phase, **rows[phase]}), flush=True)
        (destination / "result.json").write_text(json.dumps(rows, indent=2))
    finally:
        for helper in [probe, scroll_process]:
            if helper is not None:
                stop(helper)
                helper.stdin.close()
        if pixel_log is not None:
            pixel_log.close()
        if app is not None:
            stop(app)
            reader.join(timeout=5)
            log.close()
        sandbox.close()
        stop(awake)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--app", required=True)
    parser.add_argument("--herdr", required=True)
    parser.add_argument("--python", default=os.path.realpath(sys.executable))
    parser.add_argument("--results", required=True)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--modes", nargs="+", choices=["classic", "moonframe"],
                        default=["classic", "moonframe"])
    parser.add_argument("--start-round", type=int, default=1)
    parser.add_argument("--seconds", type=float, default=12)
    parser.add_argument("--trees", type=int, default=14)
    parser.add_argument("--scroll")
    parser.add_argument("--pixels")
    parser.add_argument("--scroll-only", action="store_true")
    parser.add_argument("--steady-only", action="store_true")
    parser.add_argument("--output-only", action="store_true")
    args = parser.parse_args()
    if args.rounds < args.start_round or args.start_round < 1 or args.seconds <= 0 or args.trees < 0:
        parser.error("rounds, start-round and seconds must be positive; trees must be nonnegative")
    if args.scroll_only and not args.scroll:
        parser.error("--scroll-only requires --scroll")
    if args.pixels and not args.scroll:
        parser.error("--pixels requires --scroll")
    if args.pixels and (args.steady_only or args.output_only):
        parser.error("--pixels measures scrolling, not an output-only run")
    if sum([args.scroll_only, args.steady_only, args.output_only]) > 1:
        parser.error("choose one of --scroll-only, --steady-only, or --output-only")
    for round_number in range(args.start_round, args.rounds + 1):
        modes = args.modes if round_number % 2 else list(reversed(args.modes))
        for mode in modes:
            print(f"starting {mode} round {round_number}", flush=True)
            run_case(args, mode, round_number)


if __name__ == "__main__":
    main()
