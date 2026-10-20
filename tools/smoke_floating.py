#!/usr/bin/env python3
"""Bounded real floating lifecycle and exact stacking/suppression rendering proof."""
import argparse
import json
import os
from pathlib import Path
import signal
import stat
import subprocess
import time
import uuid

from smoke_nested import ENV_KEYS, JSON_LIMIT, LOG_LIMIT, PPM_LIMIT, require, sha256, write_json
from smoke_scene import bounded_read, read_json, read_ppm


def check_fixture(mode, value):
    require(value.get("mode") == mode and value.get("ok") is True, f"{mode}: fixture failed: {value.get('error')}")
    if mode.startswith("floating-pixels"):
        require(type(value.get("ready_at_ms")) is int and value["ready_at_ms"] < 700, "scene was not ready before screenshot threshold")
        require(value["focused_tile"]["focused"] is True and value["unfocused_float"]["focused"] is False, "stacking focus precondition missing")
        require(value["unfocused_float"]["floating"] is True, "float intent missing")
        require(value["after_settle"]["geometry"] == value["unfocused_float"]["geometry"], "floating scene changed during screenshot interval")
    else:
        require(value["initial_float"]["floating"] is True and value["mode_restore"] is True and value["pan_preserves_world_rect"] is True, "floating transition missing")
        for overlay in ("maximize", "fullscreen", "minimize"):
            require(value[overlay + "_restore"]["geometry"] == value["moved_resized"]["geometry"], f"{overlay} restore geometry differs")
        require(value["retiled"]["geometry"] == value["tiled"]["geometry"] and value["retiled"]["floating"] is False, "tile restore missing")
        require(value["unmapped"]["floating"] is False and value["remapped"]["floating"] is False, "stale float survived unmap/remap")
        require(value["parent_dialog"]["floating"] is True and value["child_survives_parent"]["geometry"] == value["parent_dialog"]["geometry"], "transient lifetime invariant failed")


def check_pixels(path, fixture, deadline):
    width, height, pixels = read_ppm(path)
    magenta = bytes([255, 0, 255])
    points = []
    for y in range(height):
        require(time.monotonic() < deadline, "pixel validation deadline expired")
        for x in range(width):
            if pixels[(y * width + x) * 3:(y * width + x + 1) * 3] == magenta:
                points.append((x, y))
    suppressed = fixture["mode"] == "floating-pixels-fullscreen"
    if suppressed:
        require(not points, "floating pixels painted above focused fullscreen")
        require(fixture["focused_tile"]["fullscreen"] is True, "fullscreen suppression precondition missing")
        # The fullscreen client has a distinct 320x240 solid patch, with trusted
        # black everywhere else. This also excludes an accidentally empty frame.
        expected_colors = {bytes([0, 0, 0]), bytes([192, 128, 64])}
        require(all(pixels[i:i+3] in expected_colors for i in range(0, len(pixels), 3)), "unexpected fullscreen colors")
        require(sum(pixels[i:i+3] == bytes([192, 128, 64]) for i in range(0, len(pixels), 3)) == 320 * 240, "fullscreen client content missing")
    else:
        require(len(points) == 320 * 240, "floating client was occluded, clipped or scaled")
        left, right = min(x for x, _ in points), max(x for x, _ in points)
        top, bottom = min(y for _, y in points), max(y for _, y in points)
        require(right - left + 1 == 320 and bottom - top + 1 == 240, "floating pixels are not one complete client rectangle")
    return {"width": width, "height": height, "sha256": sha256(path), "pixel_validation": "exact-client-rectangle", "magenta_pixels": len(points), "focused_fullscreen_suppresses_float": suppressed, "unfocused_float_above_focused_tile": not suppressed}


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
        command = [str(self.binary), "--backend", "nested", "--no-config", "--socket", socket, "--exit-after-ms", "6000", "--ready-file", str(ready), "--stats-file", str(stats)]
        if mode in ("floating-pixels", "floating-pixels-fullscreen"):
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
            if mode in ("floating-pixels", "floating-pixels-fullscreen"):
                result["pixels"] = check_pixels(image, fixture_value, self.deadline)
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
    parser.add_argument("--artifacts", type=Path)
    args = parser.parse_args()
    artifacts = (args.artifacts or root / "target/smoke-floating" / uuid.uuid4().hex).resolve()
    artifacts.mkdir(parents=True, exist_ok=False)
    binary, fixture = args.binary.resolve(), args.fixture.resolve()
    runner = Runner(binary, fixture, artifacts, 30)
    summary = {"ok": False, "cases": runner.cases, "binary_sha256": sha256(binary), "fixture_sha256": sha256(fixture), "scope": "nested real floating lifecycle, parent dialog admission, saved world geometry and exact SHM stacking/fullscreen suppression pixels; excludes physical/native execution and multi-output qualification"}
    errors = []
    for mode in ("floating", "floating-pixels", "floating-pixels-fullscreen"):
        try:
            runner.run(mode)
        except Exception as error:
            errors.append(f"{mode}: {error}")
    summary["executables_unchanged"] = summary["binary_sha256"] == sha256(binary) and summary["fixture_sha256"] == sha256(fixture)
    summary["parent_environment_unchanged"] = runner.parent_env == dict(os.environ)
    summary["source_sha256"] = {name: sha256(root / "tools" / name) for name in ("smoke_floating.py", "smoke_scene.py", "smoke_nested.py")}
    summary["errors"] = errors
    summary["ok"] = not errors and summary["executables_unchanged"] and summary["parent_environment_unchanged"]
    write_json(artifacts / "summary.json", summary)
    print(json.dumps({"ok": summary["ok"], "artifacts": str(artifacts)}))
    return 0 if summary["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
