#!/usr/bin/env python3
"""Check patches against Git snapshots without touching mirrors or .upstream."""
import argparse
from pathlib import Path
import subprocess
import tempfile


def check(repo, revision, patch):
    text = patch.read_text()
    names = [line[6:] for line in text.splitlines() if line.startswith("--- a/")]
    if not names or len(names) != len(set(names)):
        raise ValueError(f"invalid or repeated source paths in {patch}")
    with tempfile.TemporaryDirectory(prefix="dnr-patch-check-") as directory:
        stage = Path(directory)
        for name in names:
            relative = Path(name)
            if relative.is_absolute() or ".." in relative.parts:
                raise ValueError(f"unsafe patch path: {name}")
            source = subprocess.run(
                ["git", "-C", str(repo), "show", f"{revision}:{name}"],
                check=True, stdout=subprocess.PIPE,
            ).stdout
            destination = stage / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(source)
        for options in (["--check"], [], ["--reverse", "--check"]):
            subprocess.run(["git", "apply", *options, str(patch)], cwd=stage, check=True)
    hunks = sum(line.startswith("@@ ") for line in text.splitlines())
    added = sum(line.startswith("+") and not line.startswith("+++") for line in text.splitlines())
    removed = sum(line.startswith("-") and not line.startswith("---") for line in text.splitlines())
    print(f"{patch.name}: {len(names)} files, {hunks} hunks, +{added}/-{removed}; {revision} apply/reverse OK")


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    for name, revision in [("deno", "abd22074e4"), ("laufey", "1fe8787")]:
        parser.add_argument(f"--{name}", type=Path, default=root.parent / "mirror" / name)
        parser.add_argument(f"--{name}-revision", default=revision)
    args = parser.parse_args()
    for name in ["deno", "laufey"]:
        check(getattr(args, name), getattr(args, f"{name}_revision"), root / "integration" / f"{name}.patch")


if __name__ == "__main__":
    main()
