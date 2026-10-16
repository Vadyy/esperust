#!/usr/bin/env python3
from pathlib import Path
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def resolution_size(value):
    aliases = {"720": (1280, 720), "1080": (1920, 1080),
               "1440": (2560, 1440), "2160": (3840, 2160), "4k": (3840, 2160)}
    value = value.lower().removesuffix("p")
    if value in aliases:
        return aliases[value]
    width, height = value.split("x")
    return int(width), int(height)


def requested_size(args):
    values = [args[i + 1] for i, arg in enumerate(args[:-1]) if arg == "--resolution"]
    return resolution_size(values[-1]) if values else (1280, 720)


def resize_window(selector, size):
    width, height = size
    lua = (f"local w = hl.get_window({json.dumps(selector)})\n"
           f"if w ~= nil then hl.dispatch(hl.dsp.window.resize({{x={width}, y={height}, window=w}})) end")
    subprocess.run(["hyprctl", "eval", lua], check=True, capture_output=True, text=True)


def float_window(process, size):
    if not os.environ.get("HYPRLAND_INSTANCE_SIGNATURE") or not shutil.which("hyprctl"):
        return None
    for _ in range(100):
        if process.poll() is not None:
            return None
        result = subprocess.run(["hyprctl", "clients", "-j"], capture_output=True, text=True)
        if result.returncode == 0:
            owned = [window for window in json.loads(result.stdout) if window.get("pid") == process.pid]
            if owned:
                selector = f"address:{owned[0]['address']}"
                lua = (f"local w = hl.get_window({json.dumps(selector)})\n"
                       "if w ~= nil then hl.dispatch(hl.dsp.window.float({action='set', window=w})) end")
                subprocess.run(["hyprctl", "eval", lua], check=True, capture_output=True, text=True)
                resize_window(selector, size)
                return selector
        time.sleep(0.1)
    return None


def main():
    args = sys.argv[1:]
    size = requested_size(args)
    with tempfile.TemporaryDirectory(prefix="esper-window-") as folder:
        ready_file = Path(folder) / "ready.json"
        environment = os.environ.copy()
        environment["ESPER_WINDOW_READY_FILE"] = str(ready_file)
        profile = environment.get("ESPER_BUILD_PROFILE", "dev")
        if profile not in ("dev", "release"):
            raise ValueError("ESPER_BUILD_PROFILE must be dev or release")
        binary = ROOT / "target" / ("debug" if profile == "dev" else "release") / "esperust"
        process = subprocess.Popen([str(binary), *args],
                                   cwd=ROOT, env=environment)
        try:
            selector = float_window(process, size)
            settled_size = None
            if selector:
                for _ in range(20):
                    result = subprocess.run(["hyprctl", "clients", "-j"],
                                            capture_output=True, text=True)
                    if result.returncode == 0:
                        client = next((window for window in json.loads(result.stdout)
                                       if window.get("pid") == process.pid), None)
                        if client:
                            settled_size = client.get("size")
                            if settled_size == list(size):
                                break
                    if process.poll() is not None:
                        break
                    time.sleep(0.05)
            ready_file.write_text(json.dumps({"logical_size": settled_size}))
            return process.wait()
        except KeyboardInterrupt:
            return 130
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=10)


if __name__ == "__main__":
    sys.exit(main())
