#!/usr/bin/env python3
"""Verify a dual-backend dnr in a real Linux desktop session, including fallback."""
import argparse
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time

SHIM = r'''
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdlib.h>
#include <string.h>
static int mode(const char* value) {
  const char* current = getenv("DNR_TEST_CEF_FAILURE");
  return current && !strcmp(current, value);
}
const char* cef_api_hash(int version, int entry) {
  if (mode("abi")) return "dnr-test-incompatible-cef";
  const char* (*real)(int, int) = dlsym(RTLD_NEXT, "cef_api_hash");
  return real(version, entry);
}
int cef_initialize(const void* args, const void* settings, void* app, void* sandbox) {
  if (mode("initialize")) return 0;
  int (*real)(const void*, const void*, void*, void*) = dlsym(RTLD_NEXT, "cef_initialize");
  return real(args, settings, app, sandbox);
}
// Lazy imports use a provider handle, so LD_PRELOAD symbol interposition alone
// cannot inject these failures. Intercept explicit lookups as well as PLT calls.
void* dlsym(void* handle, const char* name) {
  void* (*real)(void*, const char*) = dlvsym(RTLD_NEXT, "dlsym", "GLIBC_2.2.5");
  if (mode("abi") && !strcmp(name, "cef_api_hash")) return &cef_api_hash;
  if (mode("initialize") && !strcmp(name, "cef_initialize")) return &cef_initialize;
  return real(handle, name);
}
int access(const char* path, int flags) {
  if (mode("resources") && !strcmp(path, "/usr/lib/cef/icudtl.dat")) return -1;
  int (*real)(const char*, int) = dlsym(RTLD_NEXT, "access");
  return real(path, flags);
}
'''


def run(binary, args, env, log_path, success=True, markers=()):
    with log_path.open("w") as log:
        process = subprocess.Popen([str(binary), *args], env=env, stdout=log,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = process.wait(timeout=35)
            output = log_path.read_text()
            assert code == (0 if success else 78), f"{log_path.name}: exit={code}\n{output}"
            for marker in markers:
                assert marker in output, f"{log_path.name}: missing {marker}\n{output}"
            deadline = time.monotonic() + 15
            while True:
                try:
                    os.killpg(process.pid, 0)
                except ProcessLookupError:
                    break
                assert time.monotonic() < deadline, f"{log_path.name}: native children remain"
                time.sleep(0.1)
            print(f"{log_path.name}: exit={code}, children exited", flush=True)
        finally:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dnr", required=True, type=Path)
    parser.add_argument("--logs", required=True, type=Path)
    parser.add_argument("--package", type=Path, help="Optional package of examples/desktop/smoke.ts")
    args = parser.parse_args()
    binary = args.dnr.resolve(strict=True)
    root = Path(__file__).resolve().parents[1]
    app = args.package.resolve(strict=True) if args.package else root / "examples/desktop/smoke.ts"
    version = subprocess.check_output([binary, "--version"], text=True)
    assert "backend dual" in version, version
    args.logs.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="dnr-backend-test-") as temporary:
        directory = Path(temporary)
        (directory / "shim.c").write_text(SHIM)
        subprocess.run(["cc", "-shared", "-fPIC", str(directory / "shim.c"),
                        "-ldl", "-o", str(directory / "shim.so")], check=True)
        base = dict(os.environ, XDG_DATA_HOME=str(directory / "data"),
                    XDG_CACHE_HOME=str(directory / "cache"))
        base.pop("DNR_TEST_CEF_FAILURE", None)
        for name, backend, failure, expected in [
            ("default", None, None, "system-cef"),
            ("explicit-cef", "system-cef", None, "system-cef"),
            ("explicit-webview", "webview", None, "webview"),
            ("abi-fallback", "auto", "abi", "webview"),
            ("resource-fallback", "auto", "resources", "webview"),
            ("initialize-fallback", "auto", "initialize", "webview"),
            ("explicit-cef-failure", "system-cef", "abi", None),
            ("explicit-webview-with-broken-cef", "webview", "abi", "webview"),
        ]:
            env = base.copy()
            if failure:
                env.update(LD_PRELOAD=str(directory / "shim.so"), DNR_TEST_CEF_FAILURE=failure)
            command = ["--backend", backend] if backend else []
            command.extend([str(app), expected or "system-cef"])
            markers = ["DNR_GUI_OK", f"DNR_BACKEND_OK {expected}"] if expected else ["API/hash mismatch"]
            if "fallback" in name:
                markers.append("falling back to WebView")
            run(binary, command, env, args.logs / f"{name}.log", bool(expected), markers)
        broken = dict(base, LD_PRELOAD=str(directory / "shim.so"), DNR_TEST_CEF_FAILURE="abi")
        run(binary, ["--check-system-cef"], broken, args.logs / "strict-abi-check.log",
            False, ["API/hash mismatch"])
        (directory / "cli.ts").write_text('console.log("DNR_HEADLESS_OK");')
        broken.pop("DISPLAY", None)
        broken.pop("WAYLAND_DISPLAY", None)
        run(binary, [str(directory / "cli.ts")], broken, args.logs / "headless.log",
            markers=["DNR_HEADLESS_OK"])
    print("DNR_DUAL_BACKEND_OK", flush=True)


if __name__ == "__main__":
    main()
