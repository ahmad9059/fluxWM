#!/usr/bin/env python3
"""Bounded nested popup, pointer feedback, and container clipping regression suite."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import signal
import stat
import subprocess
import sys
import time
import uuid

from smoke_nested import ENV_KEYS, JSON_LIMIT, LOG_LIMIT, PPM_LIMIT, Failure, require, sha256, write_json


def bounded_read(path, limit):
    descriptor = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
    with os.fdopen(descriptor, "rb") as source:
        require(stat.S_ISREG(os.fstat(source.fileno()).st_mode), f"not a regular evidence file: {path}")
        require(os.fstat(source.fileno()).st_size <= limit, f"evidence exceeds {limit} bytes: {path}")
        data = source.read(limit + 1)
    require(len(data) <= limit, f"evidence grew beyond limit: {path}")
    return data


def read_json(path):
    return json.loads(bounded_read(path, JSON_LIMIT))


def check_pixels(path, deadline):
    """Compare aligned RGB rows against the controlled, pre-input scene."""
    data = bounded_read(path, PPM_LIMIT)
    header = data.split(b"\n", 3)
    require(len(header) == 4 and header[0] == b"P6" and header[2] == b"255", "expected strict P6/255 image")
    require(len(header[1]) <= 128, "oversized PPM header")
    dimensions = header[1].split()
    require(len(dimensions) == 2, "expected width and height")
    width, height = map(int, dimensions)
    require(320 <= width <= 8192 and 240 <= height <= 8192, "unsupported image dimensions")
    pixels = header[3]
    require(len(pixels) == width * height * 3, "PPM payload length mismatch")
    background = bytes([15, 18, 23])
    red, green, blue, yellow = (bytes(rgb) for rgb in ([255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]))
    top = red * 160 + green * 120 + background * (width - 280)
    bottom = blue * 160 + yellow * 120 + background * (width - 280)
    clear = background * width
    for y in range(height):
        require(time.monotonic() < deadline, "deadline expired during pixel validation")
        expected = top if y < 120 else bottom if y < 240 else clear
        actual = pixels[y * width * 3:(y + 1) * width * 3]
        require(actual == expected, f"container screenshot differs from exact clipped scene at row {y}")
    return {"width": width, "height": height, "sha256": sha256(path),
            "rgb_counts": {"red": 19200, "green": 14400, "blue": 19200, "yellow": 14400, "cyan": 0},
            "background_rgb": [15, 18, 23], "right_edge_x280_matches_background": True}


def check_fixture(mode, value):
    require(value.get("mode") == mode and value.get("ok") is True, f"{mode}: fixture failed: {value.get('error')}")
    if mode == "popup-lifecycle":
        for key in ("root_null_unmap_processed", "non_grabbing_popup_dismissed_on_unmap", "invalid_serial_zero_popup_dismissed", "background_keyboard_focus_preserved", "popup_dismissed_on_root_role_destroy"):
            require(value.get(key) is True, f"{mode}: missing assertion {key}")
        require(value.get("popup_keyboard_enters") == 0, "popup stole keyboard focus")
        popups = value.get("popups", [])
        require(len(popups) == 3 and all(p.get("popup_done") == 1 for p in popups), "expected exactly one done for each popup")
        require(all(popups[i].get("frames", 0) >= 1 for i in (0, 2)), "mapped popup frame evidence missing")
    elif mode == "pointer-feedback":
        for key in ("stationary_motions_after_seed", "feedback_commits"):
            require(type(value.get(key)) is int and 0 <= value[key] <= 1, f"pointer feedback exceeds bound: {key}")
        require(value.get("changing_motions_after_seed") == 0 and value.get("initial_pointer_xy") == [0, 0], "stationary pointer precondition failed")
    else:
        require(value.get("workspace_mode") == "canvas" and value.get("actual_shm_size") == [1900, 240], "container scene preconditions missing")
        require(value.get("requested_container_width") == 280, "wrong allocated width")
        for phase in ("after_resize", "after_settle"):
            windows = value.get(phase, {}).get("windows", [])
            require(len(windows) == 1, f"{phase}: expected one window")
            geometry = windows[0].get("geometry", {})
            require([geometry.get(k) for k in ("x", "y", "width")] == [0, 0, 280], f"{phase}: wrong container placement")
        proof = value.get("input_clip_proof", {})
        require(proof.get("ok") is True and proof.get("stationary_world_pointer") == [0, 0], "input clip proof missing")
        for key in ("pointer_still_inside_actual_child", "pointer_outside_container"):
            require(proof.get(key) is True, f"input clip precondition missing: {key}")
        require(proof.get("pointer_focus_after_move", "missing") is None and proof.get("child_motion_events") == 0, "pointer remained on clipped child")
        require(value.get("child_enter_xy") == [64, 0], "actual child enter missing")
        for key, expected in {
            "child_before_world_rect": {"x": -64, "y": 0, "width": 80, "height": 80},
            "child_after_world_rect": {"x": -32, "y": 0, "width": 80, "height": 80},
            "container_after_world_rect": {"x": 32, "y": 0, "width": 280},
        }.items():
            actual = proof.get(key, {})
            require(all(actual.get(k) == v for k, v in expected.items()), f"wrong input proof geometry: {key}")
        events = [entry.get("event", {}) for entry in value.get("events", [])]
        enters = [i for i, event in enumerate(events) if event.get("clip_pointer_enter_child") is True]
        leaves = [i for i, event in enumerate(events) if event.get("clip_pointer_leave_child") is True]
        require(enters and leaves and enters[0] < leaves[-1], "ordered child enter then leave evidence missing")


class Runner:
    def __init__(self, binary, fixture, artifacts, timeout):
        self.binary, self.fixture, self.artifacts = binary, fixture, artifacts
        self.parent_env = dict(os.environ)
        self.runtime = Path(self.parent_env["XDG_RUNTIME_DIR"])
        self.deadline = time.monotonic() + timeout
        self.cases = []

    def run(self, mode):
        require(time.monotonic() < self.deadline, "whole-suite deadline expired before case startup")
        started = time.monotonic()
        deadline = min(self.deadline, started + 11)
        directory = self.artifacts / mode
        directory.mkdir()
        socket = "fluxscene-" + uuid.uuid4().hex[:24]
        endpoints = [self.runtime / socket, self.runtime / (socket + ".lock"), self.runtime / "fluxwm" / (socket + ".sock")]
        require(all(not path.exists() and not path.is_symlink() for path in endpoints), "unique endpoint collision")
        ready, stats, image = (directory / name for name in ("ready.json", "stats.json", "scene.ppm"))
        command = [str(self.binary), "--backend", "nested", "--socket", socket, "--exit-after-ms", "6000", "--ready-file", str(ready), "--stats-file", str(stats)]
        if mode == "container-clip":
            command += ["--screenshot", str(image), "--screenshot-after-ms", "700"]
        command += ["--", str(self.fixture), mode, "--timeout-ms", "4500"]
        result = {"mode": mode, "command": command, "ok": False, "socket": socket,
                  "environment": {key: self.parent_env.get(key) for key in ENV_KEYS},
                  "child_environment_overrides": {"RUST_LOG": "fluxwm=info"}}
        self.cases.append(result)
        write_json(directory / "command.json", result)
        process, observed, owned = None, False, {}
        logs = [directory / "stdout.log", directory / "stderr.log"]
        try:
            with logs[0].open("wb") as stdout, logs[1].open("wb") as stderr:
                process = subprocess.Popen(command, cwd=directory, env={**self.parent_env, "RUST_LOG": "fluxwm=info"}, stdout=stdout, stderr=stderr, start_new_session=True)
                while True:
                    require(time.monotonic() < deadline, f"{mode}: process deadline expired")
                    for path, limit in [(p, LOG_LIMIT) for p in logs] + [(ready, JSON_LIMIT), (stats, JSON_LIMIT), (image, PPM_LIMIT)]:
                        if path.exists():
                            require(path.stat().st_size <= limit, f"oversized evidence: {path}")
                    for path in endpoints:
                        if path.exists() and path not in owned:
                            info = path.lstat()
                            owned[path] = (info.st_dev, info.st_ino)
                    if ready.exists() and not observed:
                        value = read_json(ready)  # Publisher persists a complete file atomically.
                        require(value.get("pid") == process.pid and value.get("wayland_display") == socket, "readiness identity mismatch")
                        require(value.get("backend") == "nested" and value.get("status") == "dispatch-ready", "unexpected readiness status")
                        require(value.get("ipc_socket") == str(endpoints[2]), "unexpected IPC socket path")
                        require(all(stat.S_ISSOCK(p.stat().st_mode) for p in (endpoints[0], endpoints[2])), "published socket absent")
                        result["readiness"] = value
                        observed = True
                    if process.poll() is not None:
                        break
                    time.sleep(0.02)
            require(process.returncode == 0, f"{mode}: compositor exit {process.returncode}; inspect stderr.log")
            require(observed, "atomic readiness not observed")
            require(all(not p.exists() and not p.is_symlink() for p in endpoints), "socket/lock/IPC endpoint leaked")
            fixture_bytes = bounded_read(logs[0], JSON_LIMIT)
            lines = [line for line in fixture_bytes.splitlines() if line.strip()]
            require(len(lines) == 1, "expected exactly one fixture JSON record")
            fixture_value = json.loads(lines[0])
            write_json(directory / "fixture.json", fixture_value)
            check_fixture(mode, fixture_value)
            result["stats"] = read_json(stats)
            require(result["stats"].get("final_windows") == 0, "managed window remained after fixture disconnect")
            if mode == "container-clip":
                result["pixels"] = check_pixels(image, self.deadline)
            require(time.monotonic() < self.deadline, "whole-suite deadline expired during evidence validation")
            result["ok"] = True
        except Exception as error:
            result["error"] = str(error)
            raise
        finally:
            if process is not None:
                # Descendants share the private process group, even if the leader exited.
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait(timeout=2)
                result["returncode"] = process.returncode
            leaks = []
            for path in endpoints:
                if path.exists() or path.is_symlink():
                    leaks.append(str(path))
                    info = path.lstat()
                    if owned.get(path) == (info.st_dev, info.st_ino):
                        path.unlink()
            result["leaked_endpoints_before_harness_cleanup"] = leaks
            result["parent_environment_unchanged"] = dict(os.environ) == self.parent_env
            result["elapsed_ms"] = round((time.monotonic() - started) * 1000)
            write_json(directory / "result.json", result)


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=root / "target/debug/fluxwm")
    parser.add_argument("--fixture", type=Path, default=root / "target/debug/fluxwm-fixture")
    parser.add_argument("--artifacts", type=Path, help="new retained evidence directory")
    parser.add_argument("--timeout", type=float, default=45, help="whole suite seconds, 20..120")
    args = parser.parse_args()
    require(20 <= args.timeout <= 120, "timeout must be 20..120 seconds")
    require(os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY"), "existing graphical host required")
    runtime = Path(os.environ.get("XDG_RUNTIME_DIR", ""))
    require(runtime.is_absolute() and runtime.is_dir(), "absolute existing XDG_RUNTIME_DIR required")
    info = runtime.lstat()
    require(stat.S_ISDIR(info.st_mode) and info.st_uid == os.getuid() and stat.S_IMODE(info.st_mode) == 0o700, "runtime directory must be owned by this user with mode 0700")
    binary, fixture = args.binary.resolve(), args.fixture.resolve()
    for path in (binary, fixture):
        require(path.is_file() and os.access(path, os.X_OK), f"missing executable prerequisite: {path}")
    artifacts = (args.artifacts or root / "target/smoke-scene" / uuid.uuid4().hex).resolve()
    artifacts.mkdir(parents=True, exist_ok=False)
    runner = Runner(binary, fixture, artifacts, args.timeout)
    summary = {"ok": False, "artifacts": str(artifacts), "cases": runner.cases,
               "binary": str(binary), "fixture": str(fixture), "binary_sha256": sha256(binary), "fixture_sha256": sha256(fixture),
               "scope": "nested synthetic SHM scene/input regressions; excludes physical input, native backend and host presentation qualification"}
    errors = []
    for mode in ("popup-lifecycle", "pointer-feedback", "container-clip"):
        try:
            runner.run(mode)
        except Exception as error:
            errors.append(f"{mode}: {error}")
    for name, path in (("binary", binary), ("fixture", fixture)):
        summary[name + "_sha256_after"] = sha256(path)
        if summary[name + "_sha256_after"] != summary[name + "_sha256"]:
            errors.append(f"{name} changed during suite")
    if time.monotonic() >= runner.deadline:
        errors.append("whole-suite deadline expired")
    summary["parent_environment_unchanged"] = dict(os.environ) == runner.parent_env
    if not summary["parent_environment_unchanged"]:
        errors.append("parent environment changed")
    summary["ok"], summary["errors"] = not errors, errors
    write_json(artifacts / "summary.json", summary)
    print(json.dumps({"ok": summary["ok"], "artifacts": str(artifacts), "errors": errors}))
    return 0 if summary["ok"] else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (Failure, OSError, ValueError) as error:
        print(json.dumps({"ok": False, "error": str(error)}), file=sys.stderr)
        sys.exit(1)
