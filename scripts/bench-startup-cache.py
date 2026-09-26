#!/usr/bin/env python3
"""Interleaved cache A/B benchmark using one v4 package. No application rebuilds."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import statistics
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--dnr", required=True, type=Path)
parser.add_argument("--dnc", required=True, type=Path)
parser.add_argument("--v4", required=True, type=Path)
parser.add_argument("--output", required=True, type=Path)
parser.add_argument("--runs", type=int, default=30)
parser.add_argument("--warmup", type=int, default=5)
args = parser.parse_args()
assert args.runs >= 2 and args.warmup >= 0
args.dnr = args.dnr.resolve()
args.dnc = args.dnc.resolve()
args.v4 = args.v4.resolve()
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=False)

def inspect(path):
    return json.loads(subprocess.check_output([args.dnc, "inspect", path, "--json"]))

assert inspect(args.v4)["manifest"]["formatVersion"] == 4

groups = {
    "v4-off": (args.v4, ["--no-code-cache", "--no-transpile-cache"]),
    "v4-emit": (args.v4, ["--no-code-cache"]),
    "v4-warm": (args.v4, []),
    "v4-cold": (args.v4, []),
}
samples = {name: [] for name in groups}
golden = None
counter = 0

def run(name, measured=False, stats=False):
    global golden, counter
    package, flags = groups[name]
    counter += 1
    cache = output / (f"cold-{counter}" if name == "v4-cold" else name)
    env = {**os.environ, "DNR_CACHE_DIR": str(cache)}
    env.pop("DNR_CACHE_STATS", None)
    env.pop("DNR_PROFILE", None)
    if stats:
        env["DNR_CACHE_STATS"] = "1"
    started = time.perf_counter_ns()
    result = subprocess.run([args.dnr, *flags, package, "--help"], env=env, capture_output=True, timeout=30)
    elapsed = (time.perf_counter_ns() - started) / 1e6
    if result.returncode:
        raise RuntimeError(result.stderr.decode(errors="replace"))
    if golden is None:
        golden = result.stdout
    assert golden == result.stdout, f"changed help output: {name}"
    stderr = result.stderr.decode(errors="replace")
    assert not any(term in stderr.lower() for term in ("permission denied", "operation not permitted", "lock warning", "failed to load extension")), stderr
    if measured:
        samples[name].append(elapsed)
    return {"stderr": stderr, "cache": str(cache), "elapsedMs": elapsed}

rng = random.Random(20260925)
order = list(groups)
for _ in range(args.warmup):
    rng.shuffle(order)
    for name in order:
        run(name)
for _ in range(args.runs):
    rng.shuffle(order)
    for name in order:
        run(name, measured=True)

diagnostics = {name: run(name, stats=True) for name in groups}
summary = {}
for name, values in samples.items():
    summary[name] = {"meanMs": statistics.mean(values), "medianMs": statistics.median(values), "stdevMs": statistics.stdev(values), "minMs": min(values), "maxMs": max(values)}
    print(name, json.dumps(summary[name]), flush=True)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

inodes = set()
allocated = logical_bytes = 0
for path in (output / "v4-warm").rglob("*"):
    if path.is_file():
        stat = path.stat()
        logical_bytes += stat.st_size
        if (stat.st_dev, stat.st_ino) not in inodes:
            allocated += stat.st_blocks * 512
            inodes.add((stat.st_dev, stat.st_ino))
report = {"platform": platform.platform(), "cwd": str(Path.cwd()), "runtime": str(args.dnr), "runtimeSha256": digest(args.dnr), "v4Sha256": digest(args.v4), "v4Bytes": args.v4.stat().st_size, "warmup": args.warmup, "runs": args.runs, "seed": 20260925, "stdoutSha256": hashlib.sha256(golden).hexdigest(), "stdoutBytes": len(golden), "summary": summary, "samples": samples, "diagnostics": diagnostics, "warmCacheLogicalBytes": logical_bytes, "warmCacheAllocatedBytes": allocated}
(output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
