#!/usr/bin/env python3
"""Bounded real-client tests for the nested compositor; Python standard library only."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import stat
import subprocess
import sys
import time
import uuid

ENV_KEYS = ("DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "XDG_RUNTIME_DIR", "WINIT_UNIX_BACKEND")
LOG_LIMIT = 4 * 1024 * 1024
JSON_LIMIT = 1024 * 1024
PPM_LIMIT = 64 * 1024 * 1024


class Failure(Exception):
    pass


def require(condition, message):
    if not condition:
        raise Failure(message)


def read_json(path):
    require(path.stat().st_size <= JSON_LIMIT, f"JSON exceeds evidence limit: {path}")
    return json.loads(path.read_text())


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def check_pixels(path, deadline=None):
    """Validate an actual P6 image, using distinctive color bounds independent of placement."""
    require(path.stat().st_size <= PPM_LIMIT, "screenshot exceeds bounded PPM limit")
    with path.open("rb") as source:
        require(source.readline() == b"P6\n", "screenshot must use P6 magic")
        dimensions = source.readline().split()
        require(len(dimensions) == 2, "PPM requires two dimensions")
        width, height = map(int, dimensions)
        require(0 < width <= 8192 and 0 < height <= 8192, "PPM dimensions outside limit")
        require(source.readline() == b"255\n", "PPM must use 8-bit channels")
        pixels = source.read(PPM_LIMIT + 1)
    require(len(pixels) == width * height * 3, "PPM pixel length mismatch")
    colors = {"red": b"\xff\0\0", "green": b"\0\xff\0", "blue": b"\0\0\xff", "yellow": b"\xff\xff\0"}
    bounds = {name: [width, height, -1, -1, 0] for name in colors}
    reverse = {rgb: name for name, rgb in colors.items()}
    for index in range(width * height):
        if deadline is not None and index % 4096 == 0:
            require(time.monotonic() < deadline, "whole-suite deadline expired during screenshot validation")
        name = reverse.get(pixels[index * 3:index * 3 + 3])
        if name is None:
            continue
        x, y = index % width, index // width
        b = bounds[name]
        b[0], b[1], b[2], b[3], b[4] = min(b[0], x), min(b[1], y), max(b[2], x), max(b[3], y), b[4] + 1
    # The fixture's first surface is unscaled 320x240, with four 160x120 quadrants.
    for name, b in bounds.items():
        require(b[4] == 160 * 120, f"{name}: expected 19200 exact pixels, got {b[4]}")
        require((b[2] - b[0] + 1, b[3] - b[1] + 1) == (160, 120), f"{name}: quadrant rectangle distorted")
    red, green, blue, yellow = (bounds[n] for n in ("red", "green", "blue", "yellow"))
    require(green[:2] == [red[0] + 160, red[1]], "green must lie right of red: horizontal orientation/channel error")
    require(blue[:2] == [red[0], red[1] + 120], "blue must lie below red: vertical orientation/channel error")
    require(yellow[:2] == [red[0] + 160, red[1] + 120], "yellow must be bottom-right")
    return {"format": "P6", "width": width, "height": height, "quadrants": bounds, "sha256": sha256(path)}


class Runner:
    def __init__(self, binary, fixture, artifacts, timeout):
        self.binary, self.fixture, self.artifacts = binary, fixture, artifacts
        self.deadline = time.monotonic() + timeout
        self.parent_env = dict(os.environ)
        self.runtime = Path(os.environ["XDG_RUNTIME_DIR"])
        self.cases = []

    def run(self, name, milliseconds, fixture=False, terminate=False, screenshot=False):
        case_dir = self.artifacts / name
        case_dir.mkdir()
        socket = "fluxsmoke-" + uuid.uuid4().hex[:24]
        ready, stats, image = (case_dir / n for n in ("ready.json", "stats.json", "first-map.ppm"))
        command = [str(self.binary), "--backend", "nested", "--socket", socket,
                   "--exit-after-ms", str(milliseconds), "--ready-file", str(ready), "--stats-file", str(stats)]
        if screenshot:
            command += ["--screenshot", str(image)]
        if fixture:
            command += ["--", str(self.fixture), "lifecycle", "--timeout-ms", "4500"]
        metadata = {"name": name, "command": command, "socket": socket,
                    "environment": {key: self.parent_env.get(key) for key in ENV_KEYS},
                    "child_environment_overrides": {"RUST_LOG": "fluxwm=info"}, "ok": False}
        self.cases.append(metadata)
        write_json(case_dir / "command.json", metadata)
        started = time.monotonic()
        process = None
        observed_ready = False
        signaled = False
        owned_paths = {}
        try:
            with (case_dir / "stdout.log").open("wb") as stdout, (case_dir / "stderr.log").open("wb") as stderr:
                child_env = dict(self.parent_env)
                child_env["RUST_LOG"] = "fluxwm=info"
                process = subprocess.Popen(command, stdout=stdout, stderr=stderr, env=child_env,
                                           cwd=case_dir, start_new_session=True)
                case_deadline = min(self.deadline, started + (12 if fixture else 8))
                while process.poll() is None:
                    require(time.monotonic() < case_deadline, f"{name}: bounded process deadline expired")
                    for log in (case_dir / "stdout.log", case_dir / "stderr.log"):
                        require(log.stat().st_size <= LOG_LIMIT, f"{name}: log exceeds evidence budget")
                    for candidate in (self.runtime / socket, self.runtime / (socket + ".lock")):
                        if candidate not in owned_paths and candidate.exists():
                            info = candidate.lstat()
                            owned_paths[candidate] = (info.st_dev, info.st_ino)
                    if ready.exists() and not observed_ready:
                        try:
                            value = read_json(ready)
                        except json.JSONDecodeError:
                            # Publisher may still be writing; retry under the same deadline.
                            value = None
                        if value is not None:
                            require(value.get("status") == "dispatch-ready" and value.get("backend") == "nested", f"{name}: unexpected readiness schema")
                            require(value.get("pid") == process.pid and value.get("wayland_display") == socket, f"{name}: readiness identity mismatch")
                            require(stat.S_ISSOCK((self.runtime / socket).stat().st_mode), f"{name}: readiness socket absent")
                            observed_ready = True
                            metadata["readiness"] = value
                            if terminate:
                                process.send_signal(signal.SIGTERM)
                                signaled = True
                    time.sleep(0.02)
                metadata["returncode"] = process.returncode
            for log in (case_dir / "stdout.log", case_dir / "stderr.log"):
                require(log.stat().st_size <= LOG_LIMIT, f"{name}: final log exceeds evidence budget")
            require(process.returncode == 0, f"{name}: compositor exited {process.returncode}; inspect stderr.log")
            require(observed_ready, f"{name}: readiness not observed before exit")
            require(not terminate or signaled, f"{name}: SIGTERM was not sent after readiness")
            require(not (self.runtime / socket).exists(), f"{name}: Wayland socket leaked")
            require(not (self.runtime / (socket + ".lock")).exists(), f"{name}: Wayland lock leaked")
            values = read_json(stats)
            metadata["stats"] = values
            require(all(type(v) is int and v >= 0 for v in values.values()), f"{name}: stats must be nonnegative integer counters")
            if fixture:
                lines = [line for line in (case_dir / "stdout.log").read_text().splitlines() if line.strip()]
                require(len(lines) == 1, f"{name}: expected exactly one fixture JSON record")
                fixture_result = json.loads(lines[0])
                write_json(case_dir / "fixture.json", fixture_result)
                require(fixture_result.get("ok") is True and fixture_result.get("mode") == "lifecycle", f"{name}: fixture failed: {fixture_result.get('error')}")
                for key, expected in {"maps": 3, "unmaps": 1, "destroyed": 2, "final_windows": 0}.items():
                    require(values.get(key) == expected, f"{name}: {key} expected {expected}, got {values.get(key)}")
                require(fixture_result.get("invalid_unmapped_move_attempted") is True, f"{name}: stale unmapped move was not exercised")
                windows = fixture_result.get("windows", [])
                require(len(windows) == 2 and windows[0].get("frame_callbacks", 0) >= 3 and windows[1].get("frame_callbacks", 0) >= 1, f"{name}: callback progress evidence incomplete")
                require(values.get("redraws", 0) >= 1 and values.get("submissions", 0) >= 1, f"{name}: no rendering/submission observed")
            else:
                require(values.get("maps") == 0 and values.get("final_windows") == 0, f"{name}: unexpected client state")
            if screenshot:
                metadata["screenshot"] = check_pixels(image, self.deadline)
            require(dict(os.environ) == self.parent_env, "harness mutated parent environment")
            metadata["ok"] = True
            return values
        except Exception as error:
            metadata["error"] = str(error)
            raise
        finally:
            if process is not None and process.poll() is None:
                # Kill only the session/process group created by this harness.
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait(timeout=2)
                metadata["forced_cleanup"] = True
            leaks = []
            for candidate in (self.runtime / socket, self.runtime / (socket + ".lock")):
                if candidate.exists():
                    leaks.append(str(candidate))
                    info = candidate.lstat()
                    # Remove only an inode observed while our private child was live.
                    if owned_paths.get(candidate) == (info.st_dev, info.st_ino):
                        candidate.unlink()
            if process is not None:
                metadata["returncode"] = process.returncode
            metadata["leaked_socket_paths_before_harness_cleanup"] = leaks
            if stats.exists() and "stats" not in metadata:
                try:
                    metadata["stats"] = read_json(stats)
                except (OSError, ValueError, Failure) as error:
                    metadata["stats_read_error"] = str(error)
            metadata["elapsed_ms"] = round((time.monotonic() - started) * 1000)
            metadata["parent_environment_unchanged"] = dict(os.environ) == self.parent_env
            write_json(case_dir / "result.json", metadata)


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=root / "target/debug/fluxwm")
    parser.add_argument("--fixture", type=Path, default=root / "target/debug/fluxwm-fixture")
    parser.add_argument("--artifacts", type=Path, help="new evidence directory; never deleted, including on failure")
    parser.add_argument("--timeout", type=float, default=40.0, help="whole-suite wall-clock budget, 10..120 seconds")
    args = parser.parse_args()
    require(10 <= args.timeout <= 120, "timeout must be 10..120 seconds")
    require("XDG_RUNTIME_DIR" in os.environ, "XDG_RUNTIME_DIR is required")
    require(os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY"), "an existing graphical host is required")
    binary, fixture = args.binary.resolve(), args.fixture.resolve()
    require(binary.is_file() and os.access(binary, os.X_OK), f"missing executable: {binary}")
    require(fixture.is_file() and os.access(fixture, os.X_OK), f"missing fixture: {fixture}")
    artifacts = (args.artifacts or root / "target/smoke-nested" / uuid.uuid4().hex).resolve()
    artifacts.mkdir(parents=True, exist_ok=False)
    runner = Runner(binary, fixture, artifacts, args.timeout)
    summary = {"ok": False, "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
               "artifacts": str(artifacts), "binary_sha256": sha256(binary),
               "fixture_sha256": sha256(fixture), "cases": runner.cases,
               "scope": "nested SHM lifecycle, copied offscreen GLES scene framebuffer, idle scheduling, SIGTERM and socket cleanup; no native/hardware/host-presentation qualification"}
    errors = []
    def collect(name, *positional, **keywords):
        try:
            return runner.run(name, *positional, **keywords)
        except Exception as error:
            errors.append(str(error))
            return None
    try:
        collect("lifecycle", 6000, fixture=True, screenshot=True)
        short = collect("idle-short", 600)
        long = collect("idle-long", 1600)
        if short is not None and long is not None:
            try:
                for key in ("redraws", "compositions", "submissions"):
                    require(short.get(key, 0) >= 1, f"idle-short: no initial {key}")
                    require(long.get(key, 0) <= short[key] + 2, f"idle work grows with duration: {key}: {short[key]} -> {long.get(key)}")
                    require(long.get(key, 0) <= 6, f"idle-long: excessive {key}: {long.get(key)}")
            except Failure as error:
                errors.append(str(error))
            summary["idle_comparison"] = {"short_ms": 600, "long_ms": 1600, "short": short, "long": long,
                                          "allowed_extra_redraws_compositions_or_submissions": 2, "maximum_long_count": 6}
        collect("sigterm", 10000, terminate=True)
        summary["binary_sha256_after"] = sha256(binary)
        summary["fixture_sha256_after"] = sha256(fixture)
        if summary["binary_sha256_after"] != summary["binary_sha256"] or summary["fixture_sha256_after"] != summary["fixture_sha256"]:
            errors.append("tested executables changed during the suite; binary provenance is not stable")
        summary["ok"] = not errors
        if errors:
            summary["error"] = "; ".join(errors)
    finally:
        summary["parent_display_environment_unchanged"] = dict(os.environ) == runner.parent_env
        write_json(artifacts / "summary.json", summary)
    print(json.dumps({"ok": summary["ok"], "artifacts": str(artifacts), "error": summary.get("error")}))
    return 0 if summary["ok"] else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (Failure, OSError, ValueError) as error:
        print(json.dumps({"ok": False, "error": str(error)}), file=sys.stderr)
        sys.exit(1)
