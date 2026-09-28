#!/usr/bin/env python3
"""Real Cmd/Ctrl+plus, restart and API persistence regression (macOS or X11).

Run with python3 scripts/test-zoom-persistence.py [--dnr dist/dnr].
On Linux use the isolated gui-run session; on macOS Automation permission is
required. Settings and the fixture are isolated in a temporary directory.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import urllib.request


def until(check, message):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(0.1)
    raise AssertionError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dnr", default="dist/dnr")
    parser.add_argument("--backend", default="webview")
    parser.add_argument("--cancel-wallet-setup", action="store_true")
    args = parser.parse_args()
    if args.cancel_wallet_setup and os.environ.get("GUI_SESSION_SLOT") != "verify":
        parser.error("wallet setup cancellation requires the isolated verify session")
    binary = str(Path(args.dnr).resolve())
    with tempfile.TemporaryDirectory(prefix="dnr-zoom-persistence-") as directory:
        root = Path(directory)
        script = root / "main.ts"
        script.write_text('''
const d = Deno.desktop;
const win = new Deno.BrowserWindow({title:"DNR Zoom Persistence",width:800,height:500});
let finish;
const done = new Promise(r => finish = r);
const server = Deno.serve({hostname:"127.0.0.1",port:0,onListen(){}}, async request => {
  const u = new URL(request.url);
  if (u.pathname === "/factor") d.setZoomFactor(Number(u.searchParams.get("v")));
  if (u.pathname === "/quit") finish();
  if (u.pathname === "/state") {
    const result = await win.executeJs("innerWidth");
    return Response.json({global:d.getGlobalZoomFactor(),app:d.getZoomFactor(),width:result.value});
  }
  return new Response("<h1>DNR Zoom Persistence</h1><p>Native shortcut and restart verification</p>",
    {headers:{"content-type":"text/html; charset=utf-8"}});
});
const page = `http://127.0.0.1:${server.addr.port}/page`;
const loaded = new Promise(r => win.addEventListener("load", async () => {
  if ((await win.executeJs("location.href")).value === page) r();
}));
win.navigate(page);
await loaded;
win.focus();
console.log(`READY http://127.0.0.1:${server.addr.port}`);
await done;
win.destroy();
await server.shutdown();
''')
        env = {**os.environ, "DNR_CONFIG_DIR": str(root / "config")}
        subprocess.run([binary, "zoom", "set", "1.25"], env=env, check=True)
        for attempt, expected in enumerate([1, 1.1, 1.5, 1]):
            log = root / f"run-{attempt}.log"
            with log.open("w") as output:
                proc = subprocess.Popen([binary, "--backend", args.backend, str(script)],
                                        env=env, stdout=output, stderr=subprocess.STDOUT)
                try:
                    last_wallet_check = 0.0
                    def ready():
                        nonlocal last_wallet_check
                        if proc.poll() is not None:
                            raise AssertionError(log.read_text())
                        if args.cancel_wallet_setup and time.monotonic() - last_wallet_check >= 1:
                            last_wallet_check = time.monotonic()
                            tree = json.loads(subprocess.check_output(["gui-tree"], text=True, timeout=5))
                            def cancel(node, wallet=False):
                                wallet = wallet or (node.get("name") == "KDE Wallet Service"
                                                    and node.get("role") == "dialog")
                                if wallet and node.get("name") == "Cancel" and node.get("showing"):
                                    x, y, width, height = node["bounds"]
                                    subprocess.run(["xdotool", "mousemove", "--sync", str(x + width // 2),
                                                    str(y + height // 2), "click", "1"], check=True)
                                    return True
                                return any(cancel(child, wallet) for child in node.get("children", []))
                            cancel(tree)
                        return next((line[6:] for line in log.read_text().splitlines()
                                     if line.startswith("READY ")), None)
                    base = until(ready, "fixture did not load")

                    def request(path):
                        return urllib.request.urlopen(base + path, timeout=5).read()

                    def check(factor):
                        state = json.loads(request("/state"))
                        return state if state["global"] == 1.25 and state["app"] == factor and abs(
                            state["width"] * 1.25 * factor - 800) <= 5 else None

                    print("RESTORED", attempt, until(lambda: check(expected), "restore/layout mismatch"))
                    if attempt == 0:
                        if sys.platform == "darwin":
                            subprocess.run(["osascript", "-e", f'''tell application "System Events"
tell (first application process whose unix id is {proc.pid})
set frontmost to true
keystroke "=" using command down
end tell
end tell'''], check=True, timeout=15)
                        else:
                            wid = subprocess.check_output(["xdotool", "search", "--sync", "--onlyvisible",
                                                           "--pid", str(proc.pid), "--name",
                                                           "^DNR Zoom Persistence$"], text=True).splitlines()[0]
                            subprocess.run(["xdotool", "windowactivate", "--sync", wid,
                                            "key", "--clearmodifiers", "ctrl+equal"], check=True)
                        print("SHORTCUT", until(lambda: check(1.1), "native shortcut failed"))
                    elif attempt in (1, 2):
                        factor = 1.5 if attempt == 1 else 1
                        request(f"/factor?v={factor}")
                        print("API", until(lambda: check(factor), "API layout mismatch"))
                    request("/quit")
                    assert proc.wait(timeout=10) == 0, log.read_text()
                except Exception:
                    print(log.read_text(), file=sys.stderr)
                    raise
                finally:
                    if proc.poll() is None:
                        proc.terminate()
                        proc.wait(timeout=10)
        print("ZOOM_PERSISTENCE_GUI_OK", sys.platform, args.backend)


if __name__ == "__main__":
    main()
