#!/usr/bin/env python3
"""Randomized release A/B for CLI startup, first usable window and warm JS calls.

Pass the dual runtime and matching directly linked single-backend runtimes.
Results include raw samples; desktop timings require an idle real GUI session.
"""
import argparse
import json
import os
from pathlib import Path
import random
import signal
import statistics
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dual", type=Path, required=True)
    parser.add_argument("--cef", type=Path, required=True)
    parser.add_argument("--webview", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=20)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    root = Path(__file__).resolve().parents[1]
    gui = args.output / "gui.ts"
    source = (root / "examples/desktop/smoke.ts").read_text()
    source = 'const started = performance.now();\n' + source
    source = source.replace('  clearTimeout(timeout);', '''
  const readyMs = performance.now() - started;
  const readyAt = Date.now();
  const hotStart = performance.now();
  for (let i = 0; i < 200; ++i) {
    const value = await win.executeJs("21 * 2");
    if (!value.ok || value.value !== 42) throw Error("Bad hot call");
  }
  console.log("DNR_TIMING " + JSON.stringify({readyAt, readyMs, hotMs: performance.now() - hotStart}));
  clearTimeout(timeout);''')
    gui.write_text(source)
    cli = args.output / "cli.ts"
    cli.write_text('console.log("DNR_CLI_OK");\n')
    variants = {"dual": args.dual.resolve(), "cef": args.cef.resolve(), "webview": args.webview.resolve()}
    cases = [("cli", name, "auto") for name in variants]
    cases += [("gui", "dual", backend) for backend in ("system-cef", "webview")]
    cases += [("gui", "cef", "system-cef"), ("gui", "webview", "webview")]
    samples = {"/".join(case): [] for case in cases}
    rng = random.Random(314)
    env = dict(os.environ, XDG_CACHE_HOME=str(args.output.resolve() / "cache"),
               XDG_DATA_HOME=str(args.output.resolve() / "data"))
    # Two warmups per case, then randomized interleaving to limit temporal bias.
    for repetition in range(-2, args.samples):
        rng.shuffle(cases)
        for kind, variant, backend in cases:
            case = "/".join((kind, variant, backend))
            command = [str(variants[variant]), "--backend", backend, str((gui if kind == "gui" else cli).resolve())]
            if kind == "gui":
                command.append(backend)
            started_wall = time.time() * 1000
            started = time.perf_counter()
            process = subprocess.Popen(command, env=env, text=True, stdout=subprocess.PIPE,
                                       stderr=subprocess.STDOUT, start_new_session=True)
            try:
                output, _ = process.communicate(timeout=35)
                elapsed = (time.perf_counter() - started) * 1000
                assert process.returncode == 0, output
                assert ("DNR_GUI_OK" if kind == "gui" else "DNR_CLI_OK") in output, output
                deadline = time.monotonic() + 15
                while True:
                    try:
                        os.killpg(process.pid, 0)
                    except ProcessLookupError:
                        break
                    assert time.monotonic() < deadline, f"native children remain: {case}"
                    time.sleep(0.05)
            finally:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait()
            record = {"processMs": elapsed}
            for line in output.splitlines():
                if line.startswith("DNR_TIMING "):
                    record.update(json.loads(line.removeprefix("DNR_TIMING ")))
            if kind == "gui":
                assert "readyMs" in record, output
                record["firstWindowMs"] = record.pop("readyAt") - started_wall
            if repetition >= 0:
                samples[case].append(record)
        print(f"round {repetition + 1}/{args.samples}", flush=True)
        (args.output / "samples.json").write_text(json.dumps(samples, indent=2) + "\n")
    summary = {case: {key: statistics.median(row[key] for row in rows)
                      for key in rows[0]} for case, rows in samples.items()}
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
