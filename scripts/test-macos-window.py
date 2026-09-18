#!/usr/bin/env python3
"""Real macOS Cmd+H, minimize-button, Dock-reopen and close regression.

Requires a logged-in graphical session and Accessibility/Automation permission
for the invoking terminal. Usage: python3 scripts/test-macos-window.py [dist/dnr]
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def apple(script):
    result = subprocess.run(
        ["osascript", "-e", 'tell application "System Events"\n' + script + '\nend tell'],
        check=True, capture_output=True, text=True, timeout=15,
    )
    return result.stdout.strip()


def until(check, message, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if check():
            return
        time.sleep(0.15)
    raise AssertionError(message)


def main():
    if sys.platform != "darwin":
        raise SystemExit("This test requires macOS")
    binary = Path(sys.argv[1] if len(sys.argv) > 1 else ROOT / "dist/dnr").resolve()
    with tempfile.TemporaryDirectory(prefix="dnr-macos-window-") as directory:
        temporary = Path(directory)
        # A unique executable name gives the unbundled test a unique Dock item.
        name = temporary.name
        executable = temporary / name
        shutil.copy2(binary, executable)
        log = temporary / "runtime.log"
        with log.open("w") as output:
            proc = subprocess.Popen(
                [str(executable), str(ROOT / "examples/desktop/macos-window-smoke.ts")],
                cwd=temporary, stdout=output, stderr=subprocess.STDOUT,
                env={**os.environ, "LAUFEY_APP_NAME": name},
            )
            target = f"first application process whose unix id is {proc.pid}"

            def app(body):
                return apple(f"tell ({target})\n{body}\nend tell")

            def click_dock():
                apple(f'tell process "Dock" to click (first UI element of list 1 whose name is "{name}")')

            try:
                until(lambda: "DNR_MACOS_WINDOW_READY" in log.read_text(), "Window failed to load", 20)
                for _ in range(2):
                    app('set frontmost to true\nkeystroke "h" using command down')
                    until(lambda: app("get visible") == "false", "Cmd+H did not hide application")
                    click_dock()
                    until(lambda: app("get visible") == "true", "Dock did not unhide application")
                    until(lambda: app("get frontmost") == "true", "Dock did not activate application")
                    app('click (first button of window 1 whose subrole is "AXMinimizeButton")')
                    until(lambda: app('get value of attribute "AXMinimized" of window 1') == "true", "Yellow button did not minimize")
                    # Allow the native minimize animation to complete before reopening.
                    time.sleep(0.5)
                    click_dock()
                    until(lambda: app('get value of attribute "AXMinimized" of window 1') == "false", "Dock did not restore minimized window")
                    assert app("count windows") == "1", "Dock created a duplicate window"
                until(lambda: log.read_text().count("DNR_MACOS_DOCK_REOPEN") >= 2, "JS Dock reopen notification was lost")
                app('click (first button of window 1 whose subrole is "AXCloseButton")')
                code = proc.wait(timeout=10)
                assert code == 0, f"Runtime exited with {code}"
                assert "DNR_MACOS_WINDOW_CLOSED" in log.read_text(), "Native close lost async work"
                print("DNR_MACOS_WINDOW_OK: Cmd+H, Dock unhide, yellow-button minimize, Dock restore (twice), JS reopen, native close")
            finally:
                if proc.poll() is None:
                    proc.terminate()
                    try:
                        proc.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        proc.kill()
                        proc.wait()
                print(log.read_text(), end="")


if __name__ == "__main__":
    main()
