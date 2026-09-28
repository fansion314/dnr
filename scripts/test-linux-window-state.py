#!/usr/bin/env python3
"""Real X11 close / release / recreation and bounded RSS evidence in the verify session."""
import argparse
import ctypes
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", choices=["webview", "system-cef"], required=True)
    parser.add_argument("--logs", type=Path, required=True)
    args = parser.parse_args()
    assert os.environ.get("GUI_SESSION_SLOT") == "verify", "use isolated GUI_SESSION_SLOT=verify"
    root = Path(__file__).resolve().parents[1]
    logs = args.logs.resolve()
    logs.mkdir(parents=True, exist_ok=True)
    libc = ctypes.CDLL(None, use_errno=True)
    assert libc.prctl(36, 1, 0, 0, 0) == 0
    env = dict(os.environ, DNR_CONFIG_DIR=str(logs / "config"), XDG_DATA_HOME=str(logs / "data"))
    log_path = logs / "runtime.log"
    results = {"backend": args.backend, "session": "KDE X11 software rendering", "checkpoints": []}
    with log_path.open("w") as log:
        process = subprocess.Popen([str(root / "dist/dnr"), "--backend", args.backend,
                                    str(root / "examples/desktop/window-state.ts"), "--interactive"],
                                   env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            deadline = time.monotonic() + 30
            while True:
                match = re.search(r"WINDOW_STATE_URL=(http://127\.0\.0\.1:\d+)", log_path.read_text())
                if match:
                    base = match[1]
                    break
                assert process.poll() is None, log_path.read_text()
                assert time.monotonic() < deadline, log_path.read_text()
                time.sleep(0.05)

            def request(path="/"):
                with urllib.request.urlopen(base + path, timeout=10) as response:
                    return json.load(response)

            def checkpoint(label, state):
                members = []
                for path in Path("/proc").glob("[0-9]*/stat"):
                    try:
                        fields = path.read_text().rsplit(")", 1)[1].split()
                        if int(fields[2]) != process.pid or fields[0] == "Z":
                            continue
                        status = (path.parent / "status").read_text()
                        rss = re.search(r"^VmRSS:\s+(\d+)", status, re.M)
                        members.append({"pid": int(path.parent.name), "rssKb": int(rss[1]) if rss else 0})
                    except (FileNotFoundError, ProcessLookupError):
                        pass
                results["checkpoints"].append({"label": label, "state": state, "processes": members,
                                                "summedRssKb": sum(p["rssKb"] for p in members)})

            initial = request()
            assert initial["visible"] and initial["token"]
            request("/allocate")
            time.sleep(0.5)
            checkpoint("allocated", request())
            subprocess.run(["gui-session", "screenshot", str(logs / "before.png")], check=True)
            rows = subprocess.check_output(["wmctrl", "-lp"], text=True).splitlines()
            matches = [row.split()[0] for row in rows if int(row.split()[2]) == process.pid]
            assert len(matches) == 1, matches
            # WM_DELETE_WINDOW exercises native close negotiation, never win.close().
            subprocess.run(["wmctrl", "-ic", matches[0]], check=True)
            deadline = time.monotonic() + 15
            while True:
                state = request()
                if state["initialNative"][5] == 0:
                    break
                assert time.monotonic() < deadline, state
                time.sleep(0.05)
            assert not state["closed"] and not state["visible"] and state["token"] is None
            time.sleep(1)
            checkpoint("released", request())
            request("/show")
            deadline = time.monotonic() + 15
            while True:
                state = request()
                if state["token"] and state["token"] != initial["token"] and state["loads"] >= 2:
                    break
                assert time.monotonic() < deadline, state
                time.sleep(0.05)
            assert state["logicalId"] == initial["logicalId"] and state["loads"] == 2
            request("/binding")
            checkpoint("recreated", request())
            subprocess.run(["gui-session", "screenshot", str(logs / "after.png")], check=True)
            request("/exit")
            assert process.wait(timeout=15) == 0, log_path.read_text()
            deadline = time.monotonic() + 15
            while True:
                try:
                    while os.waitpid(-process.pid, os.WNOHANG)[0] > 0:
                        pass
                except ChildProcessError:
                    pass
                try:
                    os.killpg(process.pid, 0)
                except ProcessLookupError:
                    break
                assert time.monotonic() < deadline, "test descendants did not exit"
                time.sleep(0.05)
            results["passed"] = True
        finally:
            (logs / "results.json").write_text(json.dumps(results, indent=2) + "\n")
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
