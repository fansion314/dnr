#!/usr/bin/env python3
"""Check dual GUI loading with libraries denied by the ELF loader (no system edits).

The headless checks also run in Arch release builds. --gui requires a real desktop
and checks delayed window creation, missing backends, fallback and process cleanup.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("backends", ROOT / "scripts/test-linux-backends.py")
backends = importlib.util.module_from_spec(spec)
spec.loader.exec_module(backends)

AUDIT = r'''
#define _GNU_SOURCE
#include <link.h>
#include <stdlib.h>
#include <string.h>
unsigned int la_version(unsigned int version) { return LAV_CURRENT; }
char* la_objsearch(const char* name, uintptr_t* cookie, unsigned int flag) {
  const char* mode = getenv("DNR_TEST_GUI_MISSING");
  if (!mode) return (char*)name;
  int cef = strstr(name, "libcef.so") != NULL;
  int webview = strstr(name, "libwebkit2gtk-") || strstr(name, "libjavascriptcoregtk-")
                || strstr(name, "libsoup-3.0");
  int common = strstr(name, "libgtk-3.") || strstr(name, "libgdk-3.")
               || strstr(name, "libglib-2.0") || strstr(name, "libX11.so");
  if ((!strcmp(mode, "all") && (cef || webview || common)) ||
      (!strcmp(mode, "cef") && cef) || (!strcmp(mode, "webview") && webview)) return NULL;
  return (char*)name;
}
'''

PROBE = '''
function checkUnloaded() {
  const maps = Deno.readTextFileSync("/proc/self/maps");
  if (/lib(?:cef|webkit2gtk|javascriptcoregtk|gtk-3|gdk-3|glib-2|X11)[-.]/.test(maps)) {
    throw new Error(`GUI libraries loaded before a desktop operation: ${maps}`);
  }
  console.log("DNR_NO_GUI_LIBRARIES");
}
checkUnloaded();
const server = Deno.serve({hostname: "127.0.0.1", port: 0}, () => new Response("ok"));
if (await (await fetch(`http://127.0.0.1:${server.addr.port}`)).text() !== "ok") throw Error("HTTP failed");
await server.shutdown();
await new Promise(resolve => setTimeout(resolve, 30));
checkUnloaded();
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dnr", required=True, type=Path)
    parser.add_argument("--gui", action="store_true")
    parser.add_argument("--logs", type=Path)
    args = parser.parse_args()
    binary = args.dnr.resolve(strict=True)
    assert "backend dual" in subprocess.check_output([binary, "--version"], text=True)
    with tempfile.TemporaryDirectory(prefix="dnr-lazy-test-") as temporary:
        directory = Path(temporary)
        logs = args.logs or directory / "logs"
        logs.mkdir(parents=True, exist_ok=True)
        (directory / "audit.c").write_text(AUDIT)
        subprocess.run(["cc", "-shared", "-fPIC", str(directory / "audit.c"),
                        "-o", str(directory / "audit.so")], check=True)
        base = dict(os.environ, LD_AUDIT=str(directory / "audit.so"),
                    XDG_DATA_HOME=str(directory / "data"), XDG_CACHE_HOME=str(directory / "cache"))
        headless = dict(base, DNR_TEST_GUI_MISSING="all")
        headless.pop("DISPLAY", None)
        headless.pop("WAYLAND_DISPLAY", None)
        cli = directory / "cli.ts"
        cli.write_text(PROBE)
        for backend in ("auto", "system-cef", "webview"):
            backends.run(binary, ["--backend", backend, str(cli)], headless,
                         logs / f"headless-{backend}.log", markers=["DNR_NO_GUI_LIBRARIES"])
        backends.run(binary, ["--check-system-cef"], headless, logs / "missing-cef-check.log",
                     False, ["cannot load GUI library"])
        if args.gui:
            app = directory / "delayed-window.ts"
            smoke = json.dumps((ROOT / "examples/desktop/smoke.ts").as_uri())
            app.write_text(PROBE + f"\nawait import({smoke});\n")
            for name, missing, backend, expected in [
                ("delayed-cef", "webview", "system-cef", "system-cef"),
                ("delayed-webview", "cef", "webview", "webview"),
                ("missing-cef-fallback", "cef", "auto", "webview"),
                ("auto-without-webview", "webview", "auto", "system-cef"),
                ("explicit-cef-missing", "cef", "system-cef", None),
                ("explicit-webview-missing", "webview", "webview", None),
                ("both-missing", "all", "auto", None),
            ]:
                markers = ["DNR_NO_GUI_LIBRARIES"]
                markers += ["DNR_GUI_OK", f"DNR_BACKEND_OK {expected}"] if expected else ["cannot load GUI library"]
                if backend == "auto" and missing in ("cef", "all"):
                    markers.append("falling back to WebView")
                backends.run(binary, ["--backend", backend, str(app), expected or "unused"],
                             dict(base, DNR_TEST_GUI_MISSING=missing), logs / f"{name}.log",
                             bool(expected), markers)
    print("DNR_LAZY_GUI_OK", flush=True)


if __name__ == "__main__":
    main()
