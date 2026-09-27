#!/usr/bin/env python3
"""Native X11 keyboard and page-layout regression; run inside gui-run.

Uses its own zoom config and the running session's display. Do not run alongside
other GUI tests. Example: GUI_SESSION_SLOT=verify gui-run python3
scripts/test-linux-zoom.py --backend webview --logs .cache/validation/zoom/webview
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import time
import urllib.request


def until(check, message, timeout=15):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        value = check()
        if value:
            return value
        time.sleep(0.05)
    raise AssertionError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dnr", default="dist/dnr")
    parser.add_argument("--backend", choices=["webview", "system-cef"], required=True)
    parser.add_argument("--logs", type=Path, required=True)
    parser.add_argument("--data", action="store_true", help="Use data URLs to diagnose HTTP navigation failures")
    parser.add_argument("--cancel-wallet-setup", action="store_true",
                        help="Cancel a first-run KWallet wizard only in GUI_SESSION_SLOT=verify")
    args = parser.parse_args()
    if args.cancel_wallet_setup and os.environ.get("GUI_SESSION_SLOT") != "verify":
        parser.error("wallet setup cancellation requires the isolated verify session")
    logs = args.logs.resolve()
    logs.mkdir(parents=True, exist_ok=True)
    binary = str(Path(args.dnr).resolve())
    env = {**os.environ, "DNR_CONFIG_DIR": str(logs / "config")}
    subprocess.run([binary, "zoom", "set", "1.25"], env=env, check=True)
    log = logs / "runtime.log"
    with log.open("w") as output:
        proc = subprocess.Popen([binary, "--backend", args.backend,
                                 "examples/desktop/zoom.ts", "--interactive", *(["--data"] if args.data else [])],
                                env=env, stdout=output, stderr=subprocess.STDOUT)
        base = None
        try:
            last_wallet_check = 0.0
            def ready():
                nonlocal last_wallet_check
                if args.cancel_wallet_setup and time.monotonic() - last_wallet_check >= 1:
                    last_wallet_check = time.monotonic()
                    tree = json.loads(subprocess.check_output(["gui-tree"], text=True, timeout=10))
                    def cancel_button(node, in_wallet=False):
                        in_wallet = in_wallet or (node.get("name") == "KDE Wallet Service" and node.get("role") == "dialog")
                        if in_wallet and node.get("name") == "Cancel" and node.get("showing"):
                            return node.get("bounds")
                        for child in node.get("children", []):
                            result = cancel_button(child, in_wallet)
                            if result:
                                return result
                    bounds = cancel_button(tree)
                    if bounds:
                        x, y, width, height = bounds
                        subprocess.run(["xdotool", "mousemove", "--sync", str(x + width // 2), str(y + height // 2), "click", "1"], check=True)
                        print("Canceled first-run KWallet wizard in isolated verify session")
                if proc.poll() is not None:
                    raise AssertionError(log.read_text())
                return re.search(r"ZOOM_READY (http://127\.0\.0\.1:\d+)", log.read_text())
            base = until(ready, "fixture startup", 45).group(1)

            def request(path):
                return urllib.request.urlopen(base + path, timeout=5).read()

            def state():
                return json.loads(request("/state"))

            def xdo(*command):
                return subprocess.check_output(["xdotool", *command], text=True).strip()

            candidates = xdo("search", "--onlyvisible", "--name", "^DNR Zoom A$").splitlines()
            wid = next(w for w in candidates if int(xdo("getwindowpid", w)) == proc.pid)
            xdo("windowactivate", "--sync", wid)
            xdo("key", "--clearmodifiers", "Tab")
            until(lambda: state()["windows"][0].get("focus") == "INPUT", "input focus")
            results = []

            def expect(factor, label):
                def matched():
                    s = state()
                    return s if abs(s["app"] - factor) < 1e-8 and all(
                        abs(w["width"] * s["effective"] - 800) <= 5
                        for w in s["windows"]) else None
                try:
                    value = until(matched, label)
                except AssertionError:
                    failure = state()
                    (logs / "failure-state.json").write_text(json.dumps(failure, indent=2))
                    print("FAILED_STATE", json.dumps(failure))
                    raise
                assert all(w["keys"] == 0 for w in value["windows"]), label + ": key leaked to page"
                results.append({"check": label, "state": value})

            for key, factor in [("ctrl+equal", 1.1), ("ctrl+plus", 1.25),
                                ("ctrl+minus", 1.1), ("ctrl+0", 1),
                                ("ctrl+KP_Add", 1.1), ("ctrl+KP_Subtract", 1),
                                ("ctrl+KP_0", 1)]:
                before = len(state()["changes"])
                previous = state()["app"]
                xdo("key", "--clearmodifiers", key)
                expect(factor, key)
                after = state()
                assert len(after["changes"]) == before + (previous != factor), "duplicate event"
            xdo("keydown", "ctrl", "keydown", "equal", "keydown", "equal", "keyup", "equal", "keyup", "ctrl")
            expect(1.1, "repeated keydown")
            request("/factor?value=2")
            expect(2, "API upper bound")
            xdo("key", "--clearmodifiers", "ctrl+equal")
            expect(2, "upper boundary")
            xdo("key", "--clearmodifiers", "ctrl+0")
            expect(1, "reset keeps global")
            subprocess.run(["gui-session", "screenshot", str(logs / "windows.png")], check=True)
            if args.backend == "system-cef":
                request("/factor?value=1.5")
                expect(1.5, "before native zoom UI")
                tree = json.loads(subprocess.check_output(["gui-tree"], text=True))
                def reset_buttons(node):
                    if node.get("name") == "Reset" and node.get("showing") and "button" in node.get("role", ""):
                        yield node
                    for child in node.get("children", []):
                        yield from reset_buttons(child)
                buttons = list(reset_buttons(tree))
                if buttons:
                    # Coordinates come from the live accessibility tree, not a
                    # guessed location. This session belongs to this test.
                    x, y, width, height = buttons[0]["bounds"]
                    xdo("mousemove", "--sync", str(x + width // 2), str(y + height // 2), "click", "1")
                    expect(1, "native Chrome Reset preserves global")
                else:
                    results.append({"check": "native zoom UI not exposed in accessibility tree"})
                    request("/factor?value=1")
                    expect(1, "restore after native UI check")

            # Global changes are persisted for future processes only.
            subprocess.run([binary, "zoom", "set", "1.5"], env=env, check=True)
            assert state()["global"] == 1.25
            probe = logs / "snapshot.ts"
            probe.write_text("console.log(Deno.desktop.getGlobalZoomFactor(), Deno.desktop.getZoomFactor())")
            new = subprocess.check_output([binary, str(probe)], env=env, text=True).strip()
            assert new == "1.5 1", new

            request("/shortcuts?enabled=false")
            before = len(state()["changes"])
            xdo("key", "--clearmodifiers", "ctrl+equal")
            time.sleep(0.2)
            disabled = state()
            assert disabled["app"] == 1 and len(disabled["changes"]) == before
            assert all(abs(w["width"] * disabled["effective"] - 800) <= 5 for w in disabled["windows"]), "engine bypassed controller"
            assert disabled["windows"][0]["keys"] == 1, "disabled shortcut did not reach page"
            results.append({"check": "shortcuts disabled", "state": disabled})
            request("/shortcuts?enabled=true")
            request("/quit")
            assert proc.wait(timeout=15) == 0, log.read_text()
            (logs / "results.json").write_text(json.dumps(results, indent=2))
            print("ZOOM_NATIVE_KEYS_OK", args.backend)
        finally:
            if proc.poll() is None:
                if base:
                    try:
                        urllib.request.urlopen(base + "/quit", timeout=2).close()
                        proc.wait(timeout=5)
                    except Exception:
                        proc.terminate()
                else:
                    proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
            print(log.read_text(), end="")


if __name__ == "__main__":
    main()
