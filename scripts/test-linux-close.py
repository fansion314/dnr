#!/usr/bin/env python3
"""Exercise compositor-initiated closes in a real KDE Wayland session."""
import argparse
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time


def qdbus(*args):
    return subprocess.check_output(
        ["qdbus6", "org.kde.KWin", *args], text=True, timeout=10
    ).strip()


def close_window(pid, directory):
    # Match only the process started by this test; do not close other app windows.
    script = directory / "close.js"
    script.write_text(
        "function close(w) {"
        f" if (w.pid === {pid}) w.closeWindow();"
        " }\n"
        # A data URL may finish loading before KWin registers the mapped window.
        "workspace.windowAdded.connect(close);\n"
        "for (const w of workspace.windowList()) close(w);\n"
    )
    name = f"dnr-native-close-{pid}"
    script_id = qdbus(
        "/Scripting", "org.kde.kwin.Scripting.loadScript", str(script), name
    )
    if int(script_id) < 0:
        raise RuntimeError("KWin could not load the temporary close script")
    try:
        qdbus(f"/Scripting/Script{script_id}", "org.kde.kwin.Script.run")
    except Exception:
        qdbus("/Scripting", "org.kde.kwin.Scripting.unloadScript", name)
        raise
    return name


def run_case(binary, app, args, expected, log_path, readiness=None, marker=None):
    with tempfile.TemporaryDirectory(prefix="dnr-native-close-") as temporary:
        directory = Path(temporary)
        # Keep test app data separate from the user's normal application data.
        env = dict(os.environ, XDG_DATA_HOME=str(directory / "data"))
        with log_path.open("w") as log:
            process = subprocess.Popen(
                [str(binary), str(app), *args],
                env=env,
                stdout=log,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            script_name = None
            try:
                deadline = time.monotonic() + 15
                if readiness:
                    while readiness not in log_path.read_text():
                        if process.poll() is not None:
                            raise AssertionError("App exited before becoming ready")
                        if time.monotonic() > deadline:
                            raise AssertionError("Window did not become ready")
                        time.sleep(0.05)
                else:
                    # Third-party packages need not expose the test readiness marker.
                    time.sleep(4)
                if process.poll() is not None:
                    raise AssertionError("App exited before the native close request")
                started = time.monotonic()
                script_name = close_window(process.pid, directory)
                # No SIGINT is sent on a successful test path.
                code = process.wait(timeout=8)
                elapsed = time.monotonic() - started
                assert code == expected, f"exit={code}, expected={expected}"
                if marker:
                    assert marker in log_path.read_text(), f"Missing {marker}"
                # CEF helpers inherit this process group. A successful exit must
                # leave no browser/renderer process behind.
                deadline = time.monotonic() + 15
                while True:
                    try:
                        os.killpg(process.pid, 0)
                    except ProcessLookupError:
                        break
                    if time.monotonic() > deadline:
                        rows = subprocess.check_output(
                            ["ps", "-eo", "pid,ppid,pgid,stat,comm"], text=True
                        ).splitlines()
                        remaining = [
                            row for row in rows[1:]
                            if int(row.split()[2]) == process.pid
                        ]
                        raise AssertionError(f"Native child processes remain: {remaining}")
                    time.sleep(0.05)
                cleanup = time.monotonic() - started
                print(
                    f"{log_path.stem}: exit={code}, close-to-exit={elapsed:.3f}s, "
                    f"all-processes-exited={cleanup:.3f}s", flush=True
                )
            finally:
                try:
                    if script_name:
                        qdbus("/Scripting", "org.kde.kwin.Scripting.unloadScript", script_name)
                finally:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dnr", required=True, type=Path)
    parser.add_argument("--package", type=Path, help="Also test an unmodified app package")
    parser.add_argument("--repeat", type=int, default=1)
    parser.add_argument("--logs", required=True, type=Path)
    options = parser.parse_args()
    if options.repeat < 1:
        parser.error("--repeat must be positive")
    binary = options.dnr.resolve(strict=True)
    package = options.package.resolve(strict=True) if options.package else None
    root = Path(__file__).resolve().parents[1]
    options.logs.mkdir(parents=True, exist_ok=True)
    for repetition in range(1, options.repeat + 1):
        for mode, code in [("deno", 0), ("node", 9), ("shutdown", 0), ("idle", 0)]:
            marker = "DNR_NATIVE_CLOSE_DELIVERED"
            if mode in ("shutdown", "idle"):
                marker = "DNR_NATIVE_CLOSE_ASYNC_OK"
            run_case(
                binary, root / "examples/desktop/native-close.ts", [mode], code,
                options.logs / f"{mode}-{repetition}.log",
                readiness="DNR_NATIVE_CLOSE_READY", marker=marker,
            )
        if package:
            run_case(binary, package, [], 0, options.logs / f"package-{repetition}.log")
    print("DNR_NATIVE_CLOSE_OK", flush=True)


if __name__ == "__main__":
    main()
