#!/usr/bin/env python3
"""Advance downstream manifests from published release assets only."""

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

import resolve_fork_version as fork
from sync_fork_release_state import sync


def main() -> None:
    sync()
    records = fork.release_records(fork.ROOT, from_environment=False)
    latest = {record["channel"]: record for record in records}
    repository = os.environ["GITHUB_REPOSITORY"]
    with tempfile.TemporaryDirectory() as directory:
        for channel, metadata in latest.items():
            version = metadata["version"]
            tag = "v" + version
            release = json.loads(subprocess.check_output(
                ["gh", "release", "view", tag, "--repo", repository, "--json", "isDraft,isPrerelease,assets,body"], text=True,
            ))
            assets = {asset["name"] for asset in release["assets"]}
            apk = f"storage-redirect-x-manager-v{version}.apk"
            module = f"storage.redirect.x-v{version}.zip"
            if release["isDraft"] or release["isPrerelease"] != (channel == "beta") or not {apk, module, "fork-release.json", "CHANGELOG.md"} <= assets:
                raise ValueError("Release is not published with complete channel assets")
            notes = Path(directory) / f"{channel}.md"
            notes.write_text(release["body"], encoding="utf-8")
            subprocess.run([
                sys.executable, str(fork.ROOT / ".github/scripts/update_manifest.py"),
                "--manifest", str(fork.ROOT / "fork-update.json"), "--channel", channel,
                "--repository", repository, "--version", version, "--tag", tag,
                "--version-code", str(metadata["version_code"]),
                "--title", "Storage Redirect X " + tag, "--apk-asset", apk, "--module-asset", module,
                "--release-notes-file", str(notes),
            ], check=True)


if __name__ == "__main__":
    main()
