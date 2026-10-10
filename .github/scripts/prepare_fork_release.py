#!/usr/bin/env python3
"""Prepare a serialized release, recovering a published release after push failure."""

import argparse
import json
import os
import re
import subprocess

import resolve_fork_version as fork
from sync_fork_release_state import sync


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag")
    args = parser.parse_args()
    # 源码固定为本轮提交，仅刷新独立发行记录。
    subprocess.run(["git", "fetch", "origin", "SRX-R"], check=True)
    if args.tag:
        sha = os.environ["GITHUB_SHA"]
        subprocess.run(["git", "merge-base", "--is-ancestor", sha, "origin/SRX-R"], check=True)
        message = subprocess.check_output(["git", "log", "-1", "--pretty=%B", sha], text=True)
        if re.search(r"\[(?:skip ci|ci skip|no ci)\]", message, re.I):
            raise SystemExit("Release tag must not point to a skip-ci commit")
    state = subprocess.check_output(["git", "show", "origin/SRX-R:.github/fork-release-state.json"], text=True)
    (fork.ROOT / fork.STATE).write_text(state, encoding="utf-8")
    recover = sync() if os.environ.get("GITHUB_EVENT_NAME") == "push" else False
    state = (fork.ROOT / fork.STATE).read_text(encoding="utf-8")
    os.environ["SRX_FORK_RELEASE_STATE"] = state
    data = fork.resolve()
    if args.tag and args.tag != "v" + data["version"]:
        raise SystemExit("Tag does not match resolved fork version")
    data["recover"] = recover
    data["release_state"] = json.dumps(json.loads(state), separators=(",", ":"))
    for key, value in data.items():
        print(f"{key}={str(value).lower() if isinstance(value, bool) else value}")


if __name__ == "__main__":
    main()
