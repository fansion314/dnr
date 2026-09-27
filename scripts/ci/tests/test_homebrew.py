import base64
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("homebrew", Path(__file__).parents[1] / "homebrew.py")
homebrew = importlib.util.module_from_spec(spec)
spec.loader.exec_module(homebrew)


class HomebrewTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.archive = Path(self.directory.name) / "dnr-0.4.2-macos-arm64.tar.gz"
        self.archive.write_bytes(b"tested archive")
        self.formula = homebrew.render("fansion314/dnr", "0.4.2", self.archive)

    def current(self, version="0.4.1"):
        return {"sha": "old-file-sha", "content": base64.b64encode(
            self.formula.replace('version "0.4.2"', f'version "{version}"').encode()
        ).decode()}

    def test_formula_pins_release_and_actual_digest(self):
        self.assertIn("/releases/download/v0.4.2/dnr-0.4.2-macos-arm64.tar.gz", self.formula)
        self.assertIn(hashlib.sha256(b"tested archive").hexdigest(), self.formula)
        self.assertIn('bin.install "bin/dnr", "bin/dnc"', self.formula)
        self.assertNotIn("@VERSION@", self.formula)

    def test_rejects_invalid_inputs_and_mismatched_archive(self):
        for repository, version in [("bad\nrepo", "0.4.2"), ("x/y", "0.4.2-rc1"),
                                    ("x/y", "0.4.3")]:
            with self.subTest(repository=repository, version=version), self.assertRaises(ValueError):
                homebrew.render(repository, version, self.archive)

    def test_new_tap_formula(self):
        with patch.object(homebrew, "gh_api", side_effect=[{"default_branch": "main"},
                          RuntimeError("Not Found (HTTP 404)"), {}]) as api:
            homebrew.update("fansion314/dnr", self.formula)
            payload = api.call_args.args[1]
            self.assertNotIn("sha", payload)
            self.assertEqual(base64.b64decode(payload["content"]).decode(), self.formula)

    def test_update_uses_default_branch_and_compare_and_swap(self):
        with patch.object(homebrew, "gh_api", side_effect=[{"default_branch": "trunk"},
                          self.current(), {}]) as api:
            homebrew.update("fansion314/dnr", self.formula)
            self.assertEqual(api.call_args.args[1]["sha"], "old-file-sha")
            self.assertEqual(api.call_args.args[1]["branch"], "trunk")

    def test_old_run_does_not_downgrade_tap(self):
        with patch.object(homebrew, "gh_api", side_effect=[{"default_branch": "main"},
                          self.current("0.10.0")]) as api:
            homebrew.update("fansion314/dnr", self.formula)
            self.assertEqual(api.call_count, 2)

    def test_exact_rerun_is_noop(self):
        with patch.object(homebrew, "gh_api", side_effect=[{"default_branch": "main"},
                          self.current("0.4.2")]) as api:
            homebrew.update("fansion314/dnr", self.formula)
            self.assertEqual(api.call_count, 2)

    def test_cannot_change_bytes_at_same_version(self):
        with patch.object(homebrew, "gh_api", side_effect=[{"default_branch": "main"},
                          self.current("0.4.2")]) as api, self.assertRaises(ValueError):
            homebrew.update("fansion314/dnr", self.formula.replace('desc "', 'desc "Changed '))
        self.assertEqual(api.call_count, 2)

    def test_read_errors_do_not_create_formula(self):
        with patch.object(homebrew, "gh_api", side_effect=[{"default_branch": "main"},
                          RuntimeError("Forbidden (HTTP 403)")]) as api, self.assertRaises(RuntimeError):
            homebrew.update("fansion314/dnr", self.formula)
        self.assertEqual(api.call_count, 2)

    def test_conflicting_write_fails(self):
        with patch.object(homebrew, "gh_api", side_effect=[{"default_branch": "main"},
                          self.current(), RuntimeError("Conflict (HTTP 409)")]), self.assertRaises(RuntimeError):
            homebrew.update("fansion314/dnr", self.formula)

    def test_revision_upgrades_same_version_without_replacing_old_url(self):
        archive = self.archive.with_name("dnr-0.4.2-macos-arm64-r1.tar.gz")
        archive.write_bytes(b"repaired package")
        repaired = homebrew.render("fansion314/dnr", "0.4.2", archive)
        self.assertIn("  revision 1\n", repaired)
        self.assertIn("macos-arm64-r1.tar.gz", repaired)
        with patch.object(homebrew, "gh_api", side_effect=[{"default_branch":"main"}, self.current("0.4.2"), {}]) as api:
            homebrew.update("fansion314/dnr", repaired)
            self.assertEqual(api.call_args.args[1]["message"], "chore(homebrew): update dnr to 0.4.2_1")
        current = {"sha":"new-sha", "content":base64.b64encode(repaired.encode()).decode()}
        with patch.object(homebrew, "gh_api", side_effect=[{"default_branch":"main"}, current]) as api:
            homebrew.update("fansion314/dnr", self.formula)
            self.assertEqual(api.call_count, 2)



if __name__ == "__main__":
    unittest.main()
