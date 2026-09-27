#!/usr/bin/env python3
"""Render a checksum-pinned binary formula, or publish it to a dedicated tap."""

import argparse
import base64
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def version_of(formula):
    match = re.search(r'^  version "(\d+\.\d+\.\d+)"$', formula, re.M)
    if not match:
        raise ValueError("expected an explicit stable version in formula")
    return tuple(map(int, match[1].split(".")))


def render(repository, version, archive):
    if not re.fullmatch(r"[\w-]+/[\w.-]+", repository):
        raise ValueError("invalid GitHub repository")
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("expected a stable version")
    if archive.name != f"dnr-{version}-macos-arm64.tar.gz":
        raise ValueError("archive name does not match version/platform")
    with archive.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return (ROOT / "packaging/homebrew/dnr.rb.in").read_text().replace(
        "@REPOSITORY@", repository
    ).replace("@VERSION@", version).replace("@SHA256@", digest)


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
    if not re.fullmatch(r"[\w-]+/homebrew-[\w.-]+", tap):
        raise ValueError("tap must be OWNER/homebrew-NAME")
    repo = gh_api(f"repos/{tap}")  # Fail early on authentication or missing repository.
    branch = repo["default_branch"]
    endpoint = f"repos/{tap}/contents/Formula/dnr.rb"
    try:
        current = gh_api(endpoint)
    except RuntimeError as error:
        if "(HTTP 404)" not in str(error):
            raise
        current = None
    payload = {"message": f"chore: update dnr to {'.'.join(map(str, version_of(formula)))}",
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


def update_git(tap, formula):
    """Use a repository-scoped SSH deploy key supplied through GIT_SSH_COMMAND."""
    if not re.fullmatch(r"[\w-]+/homebrew-[\w.-]+", tap):
        raise ValueError("tap must be OWNER/homebrew-NAME")
    incoming = version_of(formula)
    with tempfile.TemporaryDirectory(prefix="dnr-tap-") as directory:
        checkout = Path(directory) / "tap"
        subprocess.run(["git", "clone", "--depth=1", f"git@github.com:{tap}.git", str(checkout)], check=True)
        target = checkout / "Formula/dnr.rb"
        if target.exists():
            previous = target.read_text()
            if version_of(previous) > incoming or previous == formula:
                print("Tap already matches this release or has a newer version.")
                return
            if version_of(previous) == incoming:
                raise ValueError("refusing to change an already published formula version")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(formula)
        subprocess.run(["git", "-C", str(checkout), "add", "Formula/dnr.rb"], check=True)
        subprocess.run(["git", "-C", str(checkout), "-c", "user.name=github-actions[bot]",
                        "-c", "user.email=41898282+github-actions[bot]@users.noreply.github.com",
                        "commit", "-m", f"chore: update dnr to {'.'.join(map(str, incoming))}"], check=True)
        # No force: a concurrent tap commit makes the job fail safely.
        subprocess.run(["git", "-C", str(checkout), "push", "origin", "HEAD"], check=True)


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
    git_publish = commands.add_parser("update-git")
    git_publish.add_argument("tap")
    git_publish.add_argument("formula", type=Path)
    args = parser.parse_args()
    if args.command == "render":
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(render(args.repository, args.version, args.archive))
    elif args.command == "update":
        update(args.tap, args.formula.read_text())
    else:
        update_git(args.tap, args.formula.read_text())


if __name__ == "__main__":
    main()
