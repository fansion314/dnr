#!/usr/bin/env python3
"""Render a checksum-pinned binary formula, or publish it to a repository tap."""

import argparse
import base64
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def version_of(formula):
    match = re.search(r'^  version "(\d+\.\d+\.\d+)"$', formula, re.M)
    if not match:
        raise ValueError("expected an explicit stable version in formula")
    revision = re.search(r'^  revision (\d+)$', formula, re.M)
    return (*map(int, match[1].split(".")), int(revision[1]) if revision else 0)


def render(repository, version, archive):
    if not re.fullmatch(r"[\w-]+/[\w.-]+", repository):
        raise ValueError("invalid GitHub repository")
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("expected a stable version")
    match = re.fullmatch(rf"dnr-{re.escape(version)}-macos-arm64(?:-r([1-9][0-9]*))?\.tar\.gz", archive.name)
    if not match:
        raise ValueError("archive name does not match version/platform")
    with archive.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return (ROOT / "packaging/homebrew/dnr.rb.in").read_text().replace(
        "@REPOSITORY@", repository
    ).replace("@VERSION@", version).replace("@SHA256@", digest).replace(
        "@ARCHIVE@", archive.name
    ).replace("@REVISION@", f"\n  revision {match[1]}" if match[1] else "")


def gh_api(endpoint, payload=None):
    command = ["gh", "api", endpoint]
    if payload is not None:
        command += ["--method", "PUT", "--input", "-"]
    result = subprocess.run(command, input=json.dumps(payload) if payload else None,
                            text=True, capture_output=True)
    if result.returncode:
        raise RuntimeError(result.stderr.strip())
    return json.loads(result.stdout)


def update(tap, formula):
    if not re.fullmatch(r"[\w-]+/[\w.-]+", tap):
        raise ValueError("tap repository must be OWNER/REPOSITORY")
    repo = gh_api(f"repos/{tap}")  # Fail early on authentication or missing repository.
    branch = repo["default_branch"]
    endpoint = f"repos/{tap}/contents/Formula/dnr.rb"
    try:
        current = gh_api(endpoint)
    except RuntimeError as error:
        if "(HTTP 404)" not in str(error):
            raise
        current = None
    version = version_of(formula)
    label = '.'.join(map(str, version[:3])) + (f"_{version[3]}" if version[3] else "")
    payload = {"message": f"chore(homebrew): update dnr to {label}",
               "content": base64.b64encode(formula.encode()).decode(), "branch": branch}
    if current:
        previous = base64.b64decode(current["content"]).decode()
        if version_of(previous) > version_of(formula):
            print("Tap already has a newer version; leaving it unchanged.")
            return
        if previous == formula:
            print("Tap already matches this release.")
            return
        if version_of(previous) == version_of(formula):
            raise ValueError("refusing to change an already published formula version")
        payload["sha"] = current["sha"]  # Concurrent changes fail instead of being overwritten.
    gh_api(endpoint, payload)
    print(f"Updated {tap}/Formula/dnr.rb")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    generate = commands.add_parser("render")
    generate.add_argument("repository")
    generate.add_argument("version")
    generate.add_argument("archive", type=Path)
    generate.add_argument("output", type=Path)
    publish = commands.add_parser("update")
    publish.add_argument("tap")
    publish.add_argument("formula", type=Path)
    args = parser.parse_args()
    if args.command == "render":
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(render(args.repository, args.version, args.archive))
    else:
        update(args.tap, args.formula.read_text())


if __name__ == "__main__":
    main()
