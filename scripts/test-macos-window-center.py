#!/usr/bin/env python3
"""Verify native centering after a saved resize on a macOS desktop session."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


SCRIPT = r"""
const wait = (ms) => new Promise(resolve => setTimeout(resolve, ms));
const explicit = Deno.args[0] === "position";
const win = new Deno.BrowserWindow({ title: "DNR center probe", width: 760, height: 520,
  ...(explicit ? { x: 200, y: 180 } : {}) });
await wait(450);
console.log("CENTER_PROBE", JSON.stringify({ position: win.getPosition(), size: win.getSize() }));
if (Deno.args[0] === "save") {
  win.setSize(880, 600);
  await wait(5500);
}
if (explicit) {
  win.setSize(900, 620);
  await wait(450);
  console.log("POSITION_AFTER", JSON.stringify({ position: win.getPosition(), size: win.getSize() }));
}
win.destroy();
await wait(300);
Deno.exit(0);
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dnr", type=Path, default=Path(__file__).resolve().parents[1] / "dist/dnr")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("requires macOS with an active desktop session")
    binary = args.dnr.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="dnr-center-") as directory:
        script = Path(directory) / "center.ts"
        script.write_text(SCRIPT)
        env = dict(os.environ, DNR_CONFIG_DIR=str(Path(directory) / "config"))

        def run(mode):
            result = subprocess.run([str(binary), str(script), mode], env=env,
                                    text=True, capture_output=True, timeout=30)
            assert result.returncode == 0, f"{mode}: {result.stdout}\n{result.stderr}"
            return result.stdout.splitlines()

        def value(lines, marker):
            return json.loads(next(line.split(" ", 1)[1] for line in lines
                                   if line.startswith(marker + " ")))

        initial = value(run("save"), "CENTER_PROBE")
        restored = value(run("restore"), "CENTER_PROBE")
        assert initial["size"] == [760, 520] and restored["size"] == [880, 600]
        centers = [[item["position"][0] + item["size"][0] / 2,
                    item["position"][1] + item["size"][1] / 2]
                   for item in (initial, restored)]
        assert all(abs(a - b) <= 2 for a, b in zip(*centers)), centers

        lines = run("position")
        positioned = value(lines, "CENTER_PROBE")
        resized = value(lines, "POSITION_AFTER")
        assert positioned["position"] == [200, 180], positioned
        assert resized["position"] == positioned["position"] and resized["size"] == [900, 620], resized
        print("MACOS_WINDOW_CENTER_OK", json.dumps({"initial": initial, "restored": restored,
                                                   "centers": centers, "explicit": resized}))


if __name__ == "__main__":
    main()
