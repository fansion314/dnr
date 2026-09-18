#!/usr/bin/env python3
"""Compare real Worker ZIP reads; no performance assertions.

python3 scripts/bench-parallel-read.py --dnr /path/to/before /path/to/after
Add --keep /new/output/folder to retain the fixture for the Rust parallel example.
"""
import argparse
import json
from pathlib import Path
import random
import statistics
import subprocess
import tempfile


MAIN = """
const [mode, workersText, countText, sizeText, iterationsText] = Deno.args;
const workers = Number(workersText), count = Number(countText);
const size = Number(sizeText), iterations = Number(iterationsText);
if (mode === 'hot') {
  for (let i = 0; i < count; i++) {
    const file = Deno.openSync(new URL(`./assets/${i}.bin`, import.meta.url));
    file.readSync(new Uint8Array(1));
    file.close();
  }
}
const timer = setTimeout(() => { throw Error('Worker timeout'); }, 30000);
const group = Array.from({length: workers}, () =>
  new Worker(new URL('./worker.js', import.meta.url).href, {type: 'module'}));
await Promise.all(group.map(worker => new Promise((resolve, reject) => {
  worker.onmessage = () => resolve();
  worker.onerror = reject;
})));
const done = group.map(worker => new Promise((resolve, reject) => {
  worker.onmessage = ({data}) => resolve(data);
  worker.onerror = reject;
}));
const start = performance.now();
group.forEach((worker, id) => worker.postMessage({id, workers, count, size, mode, iterations}));
const counts = await Promise.all(done);
const ms = performance.now() - start;
group.forEach(worker => worker.terminate());
clearTimeout(timer);
const reads = counts.reduce((a, b) => a + b, 0);
if (reads !== (mode === 'cold' ? count : iterations)) throw Error('read count mismatch');
console.log(JSON.stringify({ms, reads}));
"""

WORKER = """
postMessage('ready');
onmessage = ({data: {id, workers, count, size, mode, iterations}}) => {
  const byte = new Uint8Array(1);
  let reads = 0;
  for (let n = id; n < (mode === 'cold' ? count : iterations); n += workers) {
    const index = n % count;
    const file = Deno.openSync(new URL(`./assets/${index}.bin`, import.meta.url));
    if (file.readSync(byte) !== 1 || byte[0] !== index) throw Error('invalid ZIP data');
    file.seekSync(size - 1, Deno.SeekMode.Start);
    if (file.readSync(byte) !== 1 || byte[0] !== index) throw Error('invalid ZIP tail');
    file.close();
    reads++;
  }
  postMessage(reads);
};
"""


def run(args, root):
    source = root / "source"
    (source / "assets").mkdir(parents=True)
    (source / "main.js").write_text(MAIN)
    (source / "worker.js").write_text(WORKER)
    size = args.size_mib * 1024 * 1024
    rng = random.Random(42)
    # Deterministic 4-bit alphabet: substantial entropy decoding, not just
    # long runs of identical bytes. Generation/packing are outside all timers.
    table = bytes(97 + (i % 16) for i in range(256))
    for index in range(args.files):
        data = bytearray(rng.randbytes(size).translate(table))
        data[0] = data[-1] = index
        (source / "assets" / f"{index}.bin").write_bytes(data)
    package = root / "app.dnp"
    subprocess.run([str(args.dnc.resolve(strict=True)), str(source), "--entry", "main.js",
                    "-o", str(package)], check=True, capture_output=True, timeout=180)
    binaries = [binary.resolve(strict=True) for binary in args.dnr]
    results = {(binary, mode, workers): [] for binary in binaries
               for mode in ("cold", "hot") for workers in (1, 2, 4, 8)}
    for sample in range(args.samples + 1):
        order = binaries if sample % 2 == 0 else list(reversed(binaries))
        for mode in ("cold", "hot"):
            for workers in (1, 2, 4, 8):
                for binary in order:
                    output = subprocess.run(
                        [str(binary), str(package), mode, str(workers), str(args.files),
                         str(size), str(args.iterations)], check=True, text=True,
                        capture_output=True, timeout=60)
                    value = json.loads(output.stdout)
                    if sample:
                        results[binary, mode, workers].append(value["ms"])
    for (binary, mode, workers), values in results.items():
        print(json.dumps({"dnr": str(binary), "mode": mode, "workers": workers,
                          "files": args.files, "size_mib": args.size_mib,
                          "samples": args.samples, "median_ms": statistics.median(values)}),
              flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dnr", type=Path, nargs="+", required=True)
    parser.add_argument("--dnc", type=Path, default=Path("dist/dnc"))
    parser.add_argument("--files", type=int, default=16)
    parser.add_argument("--size-mib", type=int, default=8)
    parser.add_argument("--iterations", type=int, default=20000)
    parser.add_argument("--samples", type=int, default=7)
    parser.add_argument("--keep", type=Path)
    args = parser.parse_args()
    if not 8 <= args.files <= 256 or min(args.size_mib, args.iterations, args.samples) <= 0:
        parser.error("files must be 8..256; size, iterations and samples must be positive")
    if args.files * args.size_mib >= 256:
        parser.error("assets must fit below the 256 MiB cache budget for hot measurements")
    if args.keep:
        args.keep.mkdir(parents=True, exist_ok=False)
        run(args, args.keep.resolve())
    else:
        with tempfile.TemporaryDirectory(prefix="dnr-parallel-") as folder:
            run(args, Path(folder))


if __name__ == "__main__":
    main()
