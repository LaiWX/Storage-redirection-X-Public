import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SCRIPTS = Path(__file__).parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))
import resolve_fork_version as fork
import sync_fork_release_state as sync_state


class ForkVersionTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / ".github").mkdir()
        (self.root / "Cargo.toml").write_text('[package]\nversion = "1.2.60"\n')
        (self.root / "fork-version.properties").write_text("patch=0\n")
        self.write("update.json", {"beta": {"version": "1.2.60-ci.36"}})
        self.write(".github/build-version-baseline.json", {"buildCounts": {"1.2.60": 36}})
        self.write(str(fork.STATE), {"schema": 1, "releases": []})
        self.environment = patch.dict(os.environ, {}, clear=False)
        self.environment.start()
        os.environ.pop("SRX_FORK_RELEASE_STATE", None)
        self.addCleanup(self.environment.stop)

    def write(self, name, value):
        (self.root / name).write_text(json.dumps(value), encoding="utf-8")

    def metadata(self, data):
        return {**{key: data[key] for key in ("version", "version_code", "upstream_version", "channel")}, "source_commit": "a" * 40}

    def test_comparison_preserves_default_semver(self):
        pairs = [
            ("1.2.60-ci.36.patch.0", "1.2.60-ci.36.patch.1"),
            ("1.2.60-ci.36.patch.2", "1.2.60-ci.36.patch.10"),
            ("1.2.60-ci.36.patch.10", "1.2.60-ci.37.patch.10"),
            ("1.2.60-ci.37.patch.10", "1.2.60-patch.10"),
            ("1.2.60-patch.10", "1.2.61-ci.1.patch.10"),
        ]
        for old, new in pairs:
            self.assertLess(fork.compare_versions(old, new), 0)
            self.assertGreater(fork.compare_versions(new, old), 0)
            self.assertEqual(fork.compare_versions(new, new), 0)

    def test_builds_and_failure_retries_do_not_allocate(self):
        before = (self.root / fork.STATE).read_bytes()
        first = fork.resolve(self.root)
        self.assertEqual(first["version"], "1.2.60-ci.36.patch.0")
        self.assertEqual(first["version_code"], 2000000)
        (self.root / "custom-change.txt").write_text("dirty worktree")
        for _ in range(3):
            self.assertEqual(first, fork.resolve(self.root))
        self.assertEqual(before, (self.root / fork.STATE).read_bytes())

    def test_success_is_idempotent_and_new_full_version_allocates_once(self):
        first = fork.resolve(self.root)
        metadata = self.metadata(first)
        fork.record_release(self.root, metadata)
        fork.record_release(self.root, metadata)
        self.assertFalse(fork.resolve(self.root)["publish"])
        self.assertEqual(fork.resolve(self.root)["version_code"], 2000000)
        (self.root / "fork-version.properties").write_text("patch=1\n")
        second = fork.resolve(self.root)
        self.assertEqual(second["version_code"], 2000001)
        self.assertEqual(second, fork.resolve(self.root))
        fork.record_release(self.root, self.metadata(second))
        self.write(".github/build-version-baseline.json", {"buildCounts": {"1.2.60": 37}})
        third = fork.resolve(self.root)
        self.assertEqual(third["version"], "1.2.60-ci.37.patch.1")
        self.assertEqual(third["version_code"], 2000002)
        fork.record_release(self.root, self.metadata(third))
        self.write("update.json", {"stable": {"version": "1.2.60"}})
        stable = fork.resolve(self.root)
        self.assertEqual(stable["version"], "1.2.60-patch.1")
        self.assertEqual(stable["channel"], "stable")
        self.assertEqual(stable["version_code"], 2000003)

    def test_unknown_baseline_and_invalid_patch_fail(self):
        self.write(".github/build-version-baseline.json", {"buildCounts": {}})
        self.write("update.json", {})
        with self.assertRaisesRegex(ValueError, "Cannot determine"):
            fork.resolve(self.root)
        (self.root / "fork-version.properties").write_text("patch=01\n")
        with self.assertRaises(ValueError):
            fork.patch_number(self.root)

    def test_record_rejects_changed_metadata_and_wrong_code(self):
        metadata = self.metadata(fork.resolve(self.root))
        with self.assertRaises(ValueError):
            fork.record_release(self.root, {**metadata, "version_code": 2000001})
        fork.record_release(self.root, metadata)
        with self.assertRaisesRegex(ValueError, "immutable"):
            fork.record_release(self.root, {**metadata, "source_commit": "b" * 40})

    def test_cli_matches_library_and_checks_full_tag(self):
        command = [sys.executable, str(SCRIPTS / "resolve_fork_version.py"), "--root", str(self.root)]
        output = subprocess.check_output(command + ["--tag", "v1.2.60-ci.36.patch.0"], text=True)
        self.assertEqual(json.loads(output), fork.resolve(self.root))
        self.assertNotEqual(subprocess.run(command + ["--tag", "v1.2.60"], capture_output=True).returncode, 0)

    def test_recovers_published_release_before_next_code_allocation(self):
        metadata = self.metadata(fork.resolve(self.root))
        release = {"id": 1, "tag_name": "v" + metadata["version"], "draft": False, "prerelease": True}
        def download(command, **kwargs):
            directory = Path(command[command.index("--dir") + 1])
            directory.mkdir()
            (directory / "fork-release.json").write_text(json.dumps(metadata))
        with patch.dict(os.environ, {"GITHUB_REPOSITORY": "example/repo"}), patch.object(sync_state.subprocess, "check_output", return_value=json.dumps([[release]])), patch.object(sync_state.subprocess, "run", side_effect=download):
            self.assertTrue(sync_state.sync(self.root))
            self.assertFalse(sync_state.sync(self.root))
        (self.root / "fork-version.properties").write_text("patch=1\n")
        self.assertEqual(fork.resolve(self.root)["version_code"], 2000001)

    def test_prepared_state_keeps_parallel_build_entrypoints_consistent(self):
        metadata = self.metadata(fork.resolve(self.root))
        state = {"schema": 1, "releases": [metadata]}
        (self.root / "fork-version.properties").write_text("patch=1\n")
        with patch.dict(os.environ, {"SRX_FORK_RELEASE_STATE": json.dumps(state)}):
            self.assertEqual(fork.resolve(self.root)["version_code"], 2000001)

    def test_manifest_channels_and_module_fields_share_version_code(self):
        path = self.root / "fork-update.json"
        def update(version, channel, code):
            subprocess.run([
                sys.executable, str(SCRIPTS / "update_manifest.py"), "--manifest", str(path),
                "--repository", "example/repo", "--version", version, "--tag", "v" + version,
                "--channel", channel, "--version-code", str(code), "--module-asset", version + ".zip",
            ], check=True)
        update("1.2.61-ci.1.patch.10", "beta", 2000001)
        update("1.2.60-patch.10", "stable", 2000000)
        manifest = fork.load_json(path)
        self.assertEqual(manifest["version"], "v1.2.61-ci.1.patch.10")
        self.assertEqual(manifest["versionCode"], 2000001)
        self.assertEqual(manifest["beta"]["versionCode"], 2000001)
        self.assertEqual(manifest["stable"]["versionCode"], 2000000)
        self.assertTrue(manifest["zipUrl"].endswith("/1.2.61-ci.1.patch.10.zip"))
        self.assertTrue(manifest["changelog"].endswith("/CHANGELOG.md"))


if __name__ == "__main__":
    unittest.main()
