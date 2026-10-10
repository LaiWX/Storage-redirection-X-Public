#!/usr/bin/env python3
"""Resolve downstream versions without advancing the upstream build counter.

This is the single entry point for Gradle, local builds and Actions. Resolution
is read-only. Only a successfully published release may be recorded.
"""

from __future__ import annotations

import argparse
import json
import os
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
STATE = Path(".github/fork-release-state.json")
NUMBER = r"(?:0|[1-9]\d*)"
BASE = rf"{NUMBER}\.{NUMBER}\.{NUMBER}"
FORK_VERSION = re.compile(rf"^({BASE})(?:-ci\.({NUMBER})\.patch\.({NUMBER})|-patch\.({NUMBER}))$")


def compare_versions(left: str, right: str) -> int:
    """SemVer precedence, including numeric prerelease identifiers."""
    def parse(value: str) -> tuple:
        match = re.fullmatch(rf"({BASE})(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?", value)
        if not match:
            raise ValueError(f"Invalid semantic version: {value}")
        pre = match[2].split(".") if match[2] else []
        if any(not part or (part.isdigit() and len(part) > 1 and part[0] == "0") for part in pre):
            raise ValueError(f"Invalid prerelease identifiers: {value}")
        return tuple(map(int, match[1].split("."))), pre

    a, ap = parse(left)
    b, bp = parse(right)
    if a != b:
        return (a > b) - (a < b)
    if not ap or not bp:
        return (not ap) - (not bp)
    for x, y in zip(ap, bp):
        if x == y:
            continue
        if x.isdigit() and y.isdigit():
            return (int(x) > int(y)) - (int(x) < int(y))
        if x.isdigit() != y.isdigit():
            return -1 if x.isdigit() else 1
        return (x > y) - (x < y)
    return (len(ap) > len(bp)) - (len(ap) < len(bp))


def load_json(path: Path) -> dict:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError(f"Expected JSON object: {path}")
    return data


def upstream_version(root: Path) -> str:
    match = re.search(r'^version\s*=\s*"([^"]+)"', (root / "Cargo.toml").read_text(encoding="utf-8"), re.M)
    if not match or not re.fullmatch(BASE, match[1]):
        raise ValueError("Cargo.toml must specify a MAJOR.MINOR.PATCH version")
    base = match[1]
    manifest = load_json(root / "update.json")
    if (manifest.get("stable") or {}).get("version") == base:
        return base
    counts = []
    baseline = load_json(root / ".github/build-version-baseline.json")
    count = baseline.get("buildCounts", {}).get(base)
    if count is not None:
        if type(count) is not int or count < 1:
            raise ValueError(f"Invalid upstream build count for {base}")
        counts.append(count)
    beta = (manifest.get("beta") or {}).get("version", "")
    match = re.fullmatch(re.escape(base) + r"-ci\.([1-9]\d*)", beta)
    if match:
        counts.append(int(match[1]))
    if not counts:
        raise ValueError(f"Cannot determine upstream baseline for {base}; merge its published baseline/manifest first")
    return f"{base}-ci.{max(counts)}"


def patch_number(root: Path) -> int:
    text = (root / "fork-version.properties").read_text(encoding="utf-8")
    matches = re.findall(rf"^patch=({NUMBER})\s*$", text, re.M)
    if len(matches) != 1:
        raise ValueError("fork-version.properties must contain exactly one nonnegative patch=N")
    return int(matches[0])


def release_records(root: Path, *, from_environment: bool = True) -> list[dict]:
    prepared = os.environ.get("SRX_FORK_RELEASE_STATE") if from_environment else None
    data = json.loads(prepared) if prepared else load_json(root / STATE)
    if data.get("schema") != 1 or not isinstance(data.get("releases"), list):
        raise ValueError("Invalid fork release state")
    records = data["releases"]
    previous = None
    for index, record in enumerate(records):
        match = FORK_VERSION.fullmatch(record["version"])
        if not match or record["version_code"] != 2000000 + index:
            raise ValueError("Release state has an invalid version or nonsequential versionCode")
        if previous and compare_versions(previous["version"], record["version"]) >= 0:
            raise ValueError("Release state is not ordered by SemVer")
        previous = record
    return records


def resolve(root: Path = ROOT) -> dict:
    upstream = upstream_version(root)
    patch = patch_number(root)
    version = f"{upstream}{'.' if '-ci.' in upstream else '-'}patch.{patch}"
    records = release_records(root)
    existing = next((item for item in records if item["version"] == version), None)
    if not existing and records:
        last = records[-1]
        if compare_versions(version, last["version"]) <= 0:
            raise ValueError("New release must move forward in SemVer precedence")
        last_match = FORK_VERSION.fullmatch(last["version"])
        if patch < int(last_match[3] or last_match[4]):
            raise ValueError("The global patch counter must not decrease")
    return {
        "version": version,
        "version_code": existing["version_code"] if existing else 2000000 + len(records),
        "upstream_version": upstream,
        "patch": patch,
        "channel": "beta" if "-ci." in upstream else "stable",
        "publish": existing is None,
    }


def record_release(root: Path, metadata: dict) -> None:
    """Record immutable metadata downloaded from an already published release."""
    records = release_records(root, from_environment=False)
    version = metadata["version"]
    match = FORK_VERSION.fullmatch(version)
    if not match or not re.fullmatch(r"[0-9a-f]{40}", metadata.get("source_commit", "")):
        raise ValueError("Invalid release version or source commit")
    expected_upstream = f"{match[1]}-ci.{match[2]}" if match[2] else match[1]
    if metadata.get("upstream_version") != expected_upstream or metadata.get("channel") != ("beta" if match[2] else "stable"):
        raise ValueError("Release metadata baseline/channel mismatch")
    existing = next((item for item in records if item["version"] == version), None)
    if existing:
        if existing != metadata:
            raise ValueError("Published release metadata is immutable")
        return
    if metadata["version_code"] != 2000000 + len(records):
        raise ValueError("Release versionCode must use the next reserved number")
    if records:
        previous = FORK_VERSION.fullmatch(records[-1]["version"])
        if compare_versions(version, records[-1]["version"]) <= 0 or int(match[3] or match[4]) < int(previous[3] or previous[4]):
            raise ValueError("Release must advance SemVer without resetting patch")
    records.append(metadata)
    (root / STATE).write_text(json.dumps({"schema": 1, "releases": records}, indent=2) + "\n", encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--format", choices=("json", "github"), default="json")
    parser.add_argument("--tag", help="Validate a v<full-version> tag")
    parser.add_argument("--metadata", type=Path, help="Write release provenance (does not change state)")
    parser.add_argument("--source-commit")
    parser.add_argument("--record", type=Path, help="Record provenance after successful publication")
    args = parser.parse_args()
    try:
        if args.record:
            record_release(args.root, load_json(args.record))
            return
        data = resolve(args.root)
        if args.tag and args.tag != "v" + data["version"]:
            raise ValueError(f"Tag {args.tag} does not match {data['version']}")
        if args.metadata:
            if not re.fullmatch(r"[0-9a-f]{40}", args.source_commit or ""):
                raise ValueError("--metadata requires --source-commit SHA")
            metadata = {key: data[key] for key in ("version", "version_code", "upstream_version", "channel")}
            metadata["source_commit"] = args.source_commit
            args.metadata.parent.mkdir(parents=True, exist_ok=True)
            args.metadata.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
        if args.format == "github":
            for key, value in data.items():
                print(f"{key}={str(value).lower() if isinstance(value, bool) else value}")
        else:
            print(json.dumps(data, indent=2))
    except (ValueError, KeyError, OSError, TypeError) as error:
        parser.exit(1, f"Fork version error: {error}\n")


if __name__ == "__main__":
    main()
