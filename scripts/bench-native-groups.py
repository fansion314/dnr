#!/usr/bin/env python3
"""Interleaved release-runtime comparison using native_performance fixtures."""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--before", type=Path, required=True)
parser.add_argument("--after", type=Path, required=True)
parser.add_argument("--fixtures", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--samples", type=int, default=5)
args = parser.parse_args()
if args.samples < 3:
    parser.error("use at least three measured samples")
bins = {"before": args.before.resolve(), "after": args.after.resolve()}
fixtures = args.fixtures.resolve()
results = {}
cases = [("v3_plain", "plain-v3.dnp", 0), ("v3_0_groups", "groups-0.dnp", 0),
         ("v3_100_groups", "groups-100.dnp", 100),
         ("v3_1000_groups", "groups-1000.dnp", 1000)]
for case, package, groups in cases:
    samples = {name: {"outside_stat_ms": [], "stat_20k_ms": [], "total_ms": []} for name in bins}
    # Warm once, then alternate ordering to reduce scheduling/cache bias.
    for sample in range(args.samples + 1):
        order = ["before", "after"] if sample % 2 == 0 else ["after", "before"]
        for name in order:
            command = [str(bins[name]), str(fixtures / package), str(groups), "stat"]
            start = time.perf_counter()
            proc = subprocess.run(command, cwd=fixtures, capture_output=True, text=True,
                                  env={**os.environ, "DNR_CACHE_DIR": str(fixtures / "runtime-cache")},
                                  timeout=120, check=True)
            elapsed = (time.perf_counter() - start) * 1000
            stat = float(proc.stdout.strip())
            if sample:
                samples[name]["outside_stat_ms"].append(elapsed - stat)
                samples[name]["total_ms"].append(elapsed)
                samples[name]["stat_20k_ms"].append(stat)
    results[case] = {
        name: {**data, "outside_stat_median_ms": statistics.median(data["outside_stat_ms"]),
               "total_median_ms": statistics.median(data["total_ms"]),
               "stat_median_ms": statistics.median(data["stat_20k_ms"])}
        for name, data in samples.items()
    }
    print(case, json.dumps({name: {k: round(v, 3) for k, v in data.items() if "median" in k}
                           for name, data in results[case].items()}), flush=True)
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps(results, indent=2) + "\n")
