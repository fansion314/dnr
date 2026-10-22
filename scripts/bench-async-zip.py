#!/usr/bin/env python3
"""Measure cold and resident async ZIP reads with an event-loop heartbeat."""
import argparse
import json
import os
from pathlib import Path
import random
import statistics
import subprocess


SCRIPT = r"""
const source = new URL('./data.bin', import.meta.url);
let previous = performance.now(), largestGap = 0, ticks = 0;
const timer = setInterval(() => {
  const now = performance.now();
  largestGap = Math.max(largestGap, now - previous);
  previous = now;
  ticks++;
}, 2);
await new Promise(resolve => setTimeout(resolve, 20));
async function measure() {
  ticks = 0; largestGap = 0; previous = performance.now();
  const started = performance.now();
  const bytes = await Deno.readFile(source);
  const readMs = performance.now() - started;
  const ticksDuringRead = ticks;
  if (bytes.length !== 32 * 1024 * 1024 || bytes[0] !== 42 || bytes.at(-1) !== 42) {
    throw Error('incorrect ZIP read');
  }
  await new Promise(resolve => setTimeout(resolve, 10));
  return { readMs, maxGapMs: largestGap, ticksDuringRead };
}
const cold = await measure();
const resident = await measure();
clearInterval(timer);
console.log(JSON.stringify({cold, resident}));
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dnr", type=Path, required=True)
    parser.add_argument("--dnc", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=7)
    args = parser.parse_args()
    if args.samples < 2:
        parser.error("samples must be at least 2")
    dnr, dnc = args.dnr.resolve(strict=True), args.dnc.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    source = output / "source"
    source.mkdir()
    (source / "main.mjs").write_text(SCRIPT)
    table = bytes(97 + i % 16 for i in range(256))
    data = bytearray(random.Random(42).randbytes(32 * 1024 * 1024).translate(table))
    data[0] = data[-1] = 42
    (source / "data.bin").write_bytes(data)
    package = output / "app.dnp"
    subprocess.run([dnc, source, "--entry", "main.mjs", "-o", package],
                   check=True, capture_output=True, timeout=180)

    results = []
    for sample in range(args.samples + 1):
        env = {**os.environ, "DNR_CACHE_DIR": str(output / f"cache-{sample}")}
        process = subprocess.run(
            [dnr, "--no-code-cache", "--no-transpile-cache", package],
            env=env, check=True, capture_output=True, text=True, timeout=60,
        )
        value = json.loads(process.stdout)
        if sample:
            results.append(value)
    metrics = {
        phase: {
            field: statistics.median(sample[phase][field] for sample in results)
            for field in ("readMs", "maxGapMs", "ticksDuringRead")
        }
        for phase in ("cold", "resident")
    }
    report = {"dnr": str(dnr), "dnc": str(dnc), "package": str(package),
              "samples": args.samples, "fileMiB": 32, "median": metrics,
              "runs": results}
    (output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(metrics))


if __name__ == "__main__":
    main()
