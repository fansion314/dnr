#!/usr/bin/env python3
"""Compare warm ZIP readFile costs; use separate processes and alternate hosts.

python3 scripts/bench-read-file.py --dnr /path/to/before /path/to/after
No timing assertions; temporary fixtures are removed on exit.
"""
import argparse
import json
from pathlib import Path
import statistics
import subprocess
import tempfile


SCRIPT = """
const mode = Deno.args[0];
const iterations = Number(Deno.args[1]);
const file = new URL('./asset.bin', import.meta.url);
// Warm the complete ZIP entry without allocating a full readFile result.
const handle = Deno.openSync(file);
handle.readSync(new Uint8Array(1));
const size = handle.statSync().size;
handle.close();
let checksum = 0;
function consume(bytes) {
  if (bytes.length !== size || bytes[0] !== 120 || bytes[size - 1] !== 120)
    throw Error('incorrect result');
  checksum += bytes[size >> 1];
}
const start = performance.now();
if (mode === 'sync') {
  for (let n = 0; n < iterations; n++) consume(Deno.readFileSync(file));
} else {
  for (let n = 0; n < iterations; n++) consume(await Deno.readFile(file));
}
console.log(JSON.stringify({ ms: (performance.now() - start) / iterations, checksum }));
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dnr", nargs="+", required=True, type=Path)
    parser.add_argument("--dnc", type=Path, default=Path("dist/dnc"))
    parser.add_argument("--size-mib", type=int, default=16)
    parser.add_argument("--iterations", type=int, default=8)
    parser.add_argument("--samples", type=int, default=7)
    args = parser.parse_args()
    if min(args.size_mib, args.iterations, args.samples) <= 0:
        parser.error("size, iterations and samples must be positive")
    binaries = [path.resolve(strict=True) for path in args.dnr]
    dnc = args.dnc.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="dnr-read-file-") as folder:
        root = Path(folder)
        source = root / "source"
        source.mkdir()
        (source / "main.ts").write_text(SCRIPT)
        with (source / "asset.bin").open("wb") as output:
            for _ in range(args.size_mib):
                output.write(b"x" * (1024 * 1024))
        package = root / "app.dnp"
        subprocess.run([str(dnc), str(source), "--entry", "main.ts", "-o", str(package)],
                       check=True, capture_output=True, timeout=120)
        results = {(binary, mode): [] for binary in binaries for mode in ("sync", "async")}
        # One discarded round; alternate binary order to reduce ordering bias.
        for sample in range(args.samples + 1):
            order = binaries if sample % 2 == 0 else list(reversed(binaries))
            for binary in order:
                for mode in ("sync", "async"):
                    output = subprocess.run(
                        [str(binary), str(package), mode, str(args.iterations)],
                        text=True, capture_output=True, timeout=60, check=True)
                    result = json.loads(output.stdout)
                    assert result["checksum"] == 120 * args.iterations
                    if sample:
                        results[binary, mode].append(result["ms"])
        for (binary, mode), values in results.items():
            print(json.dumps({"dnr": str(binary), "mode": mode,
                              "size_mib": args.size_mib, "iterations": args.iterations,
                              "samples": args.samples,
                              "median_ms_per_read": statistics.median(values)}))


if __name__ == "__main__":
    main()
