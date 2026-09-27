"""Exercise publication ordering/failures without writing to GitHub."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]


class PublishMacosTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        for directory in ["scripts/ci", "packaging/homebrew", "docs/releases", "release-assets",
                          "fake-bin", "remote"]:
            (self.root / directory).mkdir(parents=True)
        for filename in ["scripts/ci/publish-macos.sh", "scripts/ci/homebrew.py",
                         "packaging/homebrew/dnr.rb.in"]:
            shutil.copy(ROOT / filename, self.root / filename)
        (self.root / "docs/releases/v0.4.2.md").write_text("Release notes")
        (self.root / "Cargo.toml").write_text('[workspace.package]\nversion = "0.4.2"\n')
        self.archive = self.root / "release-assets/dnr-0.4.2-macos-arm64.tar.gz"
        self.archive.write_bytes(b"tested package")
        self.checksum = self.archive.with_suffix(".gz.sha256")
        self.checksum.write_text(f"{hashlib.sha256(self.archive.read_bytes()).hexdigest()}  {self.archive.name}\n")
        subprocess.run(["python3", "scripts/ci/homebrew.py", "render", "owner/dnr", "0.4.2",
                        str(self.archive), "release-assets/dnr.rb"], cwd=self.root, check=True)
        self.env = dict(os.environ, GITHUB_REF_NAME="v0.4.2", GITHUB_REPOSITORY="owner/dnr",
                        GITHUB_SHA="a" * 40, FAKE_REMOTE_COMMIT="a" * 40,
                        FAKE_ROOT=str(self.root), FAKE_EXISTS="1",
                        PATH=str(self.root / "fake-bin") + os.pathsep + os.environ["PATH"])
        self.fake("git", '''#!/usr/bin/env python3
import os, sys
if sys.argv[1] == "ls-remote": print(os.environ["FAKE_REMOTE_COMMIT"] + "\\trefs/tags/v0.4.2")
elif sys.argv[1] == "rev-parse": print(os.environ["FAKE_REMOTE_COMMIT"])
elif sys.argv[1] == "merge-base": sys.exit(0 if os.environ.get("FAKE_ANCESTOR", "1") == "1" else 1)
''')
        self.fake("gh", '''#!/usr/bin/env python3
import json, os, pathlib, shutil, sys
root = pathlib.Path(os.environ["FAKE_ROOT"])
args = sys.argv[1:]
with (root / "calls.jsonl").open("a") as stream:
    stream.write(json.dumps(args) + "\\n")
if args[:2] == ["release", "create"]:
    (root / "created").touch()
elif args[:2] == ["release", "view"]:
    if os.environ["FAKE_EXISTS"] == "0" and not (root / "created").exists():
        sys.exit(1)
    if "--json" in args:
        print("\\n".join(p.name for p in (root / "remote").iterdir()))
elif args[:2] == ["release", "download"]:
    name = args[args.index("--pattern") + 1]
    shutil.copy(root / "remote" / name, pathlib.Path(args[args.index("--dir") + 1]) / name)
''')

    def fake(self, name, content):
        path = self.root / "fake-bin" / name
        path.write_text(content)
        path.chmod(0o755)

    def run_publish(self):
        return subprocess.run(["bash", "scripts/ci/publish-macos.sh"], cwd=self.root,
                              env=self.env, text=True, capture_output=True)

    def calls(self):
        path = self.root / "calls.jsonl"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def test_new_release_uploads_exactly_one_package_and_metadata(self):
        self.env["FAKE_EXISTS"] = "0"
        result = self.run_publish()
        self.assertEqual(result.returncode, 0, result.stderr)
        uploads = [call for call in self.calls() if call[1] == "upload"]
        self.assertEqual({Path(call[3]).name for call in uploads},
                         {self.archive.name, self.checksum.name, "dnr.rb"})
        self.assertEqual(self.calls()[-1][1], "edit")
        self.assertNotIn("--clobber", str(self.calls()))

    def test_exact_rerun_does_not_upload(self):
        for path in (self.root / "release-assets").iterdir():
            shutil.copy(path, self.root / "remote" / path.name)
        result = self.run_publish()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(any(call[1] == "upload" for call in self.calls()))

    def test_mismatching_published_archive_fails_without_writes(self):
        (self.root / "remote" / self.archive.name).write_bytes(b"already published different bytes")
        result = self.run_publish()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(call[1] in ["upload", "edit", "create"] for call in self.calls()))

    def test_corrupted_archive_fails_before_github(self):
        self.archive.write_bytes(b"corrupted")
        self.assertNotEqual(self.run_publish().returncode, 0)
        self.assertEqual(self.calls(), [])

    def test_moved_tag_fails_before_github(self):
        self.env["FAKE_REMOTE_COMMIT"] = "b" * 40
        self.assertNotEqual(self.run_publish().returncode, 0)
        self.assertEqual(self.calls(), [])

    def test_wrong_formula_fails_before_github(self):
        (self.root / "release-assets/dnr.rb").write_text("wrong formula")
        self.assertNotEqual(self.run_publish().returncode, 0)
        self.assertEqual(self.calls(), [])

    def revision(self):
        self.env.update(DNR_PACKAGE_REVISION="1", GITHUB_EVENT_NAME="workflow_dispatch", GITHUB_SHA="b"*40)
        self.archive.rename(self.archive.with_name("dnr-0.4.2-macos-arm64-r1.tar.gz"))
        self.archive = self.archive.with_name("dnr-0.4.2-macos-arm64-r1.tar.gz")
        self.checksum = self.archive.with_suffix(".gz.sha256")
        self.checksum.write_text(f"{hashlib.sha256(self.archive.read_bytes()).hexdigest()}  {self.archive.name}\n")
        subprocess.run(["python3","scripts/ci/homebrew.py","render","owner/dnr","0.4.2",
                        str(self.archive),"release-assets/dnr.rb"],cwd=self.root,check=True)

    def test_revision_publishes_new_assets_without_moving_tag(self):
        self.revision()
        result = self.run_publish()
        self.assertEqual(result.returncode, 0, result.stderr)
        uploads = [call for call in self.calls() if call[1] == "upload"]
        self.assertEqual({Path(call[3]).name for call in uploads},
                         {self.archive.name, self.checksum.name, "dnr-macos-r1.rb"})
        self.assertFalse(any(call[1] == "create" for call in self.calls()))

    def test_revision_rejects_unrelated_source_commit(self):
        self.revision()
        self.env["FAKE_ANCESTOR"] = "0"
        self.assertNotEqual(self.run_publish().returncode, 0)
        self.assertEqual(self.calls(), [])


if __name__ == "__main__":
    unittest.main()
