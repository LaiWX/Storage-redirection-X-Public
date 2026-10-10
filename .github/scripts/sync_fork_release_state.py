#!/usr/bin/env python3
"""Recover published provenance before allocating a code or updating manifests."""

import json
import os
import subprocess
import tempfile
from pathlib import Path

import resolve_fork_version as fork


def sync(root: Path = fork.ROOT) -> bool:
    repository = os.environ["GITHUB_REPOSITORY"]
    pages = json.loads(subprocess.check_output(
        ["gh", "api", "--paginate", "--slurp", f"repos/{repository}/releases?per_page=100"], text=True,
    ))
    known = {record["version"] for record in fork.release_records(root, from_environment=False)}
    missing = []
    with tempfile.TemporaryDirectory() as directory:
        for release in (item for page in pages for item in page):
            tag = release["tag_name"]
            if release["draft"] or not tag.startswith("v") or not fork.FORK_VERSION.fullmatch(tag[1:]) or tag[1:] in known:
                continue
            target = Path(directory) / str(release["id"])
            subprocess.run(["gh", "release", "download", tag, "--repo", repository, "--pattern", "fork-release.json", "--dir", str(target)], check=True)
            metadata = fork.load_json(target / "fork-release.json")
            if metadata["version"] != tag[1:] or release["prerelease"] != (metadata["channel"] == "beta"):
                raise ValueError("Published tag/channel does not match provenance")
            missing.append(metadata)
    for metadata in sorted(missing, key=lambda item: item["version_code"]):
        fork.record_release(root, metadata)
    return bool(missing)


if __name__ == "__main__":
    sync()
