#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0

"""Prepare or resume a synchronized Envshare release version."""

from __future__ import annotations

import datetime
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SEMVER = re.compile(r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$")
WORKSPACE_VERSION = re.compile(
    r'(\[workspace\.package\][\s\S]*?^version = ")([^"]+)(")', re.MULTILINE
)


def fail(message: str) -> None:
    raise SystemExit(f"prepare-release: {message}")


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, content: str) -> None:
    (ROOT / path).write_text(content, encoding="utf-8")


def replace(path: str, old: str, new: str, expected: int | None = 1) -> None:
    content = read(path)
    count = content.count(old)
    if expected is None and count == 0:
        fail(f"expected at least one occurrence of {old!r} in {path}")
    if expected is not None and count != expected:
        fail(f"expected {expected} occurrence(s) of {old!r} in {path}, found {count}")
    write(path, content.replace(old, new))


def current_version() -> str:
    match = WORKSPACE_VERSION.search(read("Cargo.toml"))
    if match is None or SEMVER.fullmatch(match.group(2)) is None:
        fail("workspace version must be a stable semantic version")
    return match.group(2)


def tag_exists(version: str) -> bool:
    result = subprocess.run(
        ["git", "rev-parse", "--quiet", "--verify", f"refs/tags/v{version}"],
        cwd=ROOT,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return result.returncode == 0


def bump_version(version: str, bump: str) -> str:
    major, minor, patch = (int(part) for part in version.split("."))
    if bump == "major":
        return f"{major + 1}.0.0"
    if bump == "minor":
        return f"{major}.{minor + 1}.0"
    if bump == "patch":
        return f"{major}.{minor}.{patch + 1}"
    fail("bump must be patch, minor, or major")


def release_commits(version: str) -> list[str]:
    result = subprocess.run(
        ["git", "log", "--reverse", "--format=%s", f"v{version}..HEAD"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    commits = [line.strip() for line in result.stdout.splitlines() if line.strip()]
    commits = [
        subject
        for subject in commits
        if not subject.startswith("chore(release): prepare v")
    ]
    if not commits:
        fail(f"no releasable commits exist after v{version}")
    return commits


def changelog_entries(commits: list[str]) -> str:
    groups: dict[str, list[str]] = {"Added": [], "Fixed": [], "Changed": []}
    conventional = re.compile(
        r"^(?P<kind>[a-z]+)(?:\((?P<scope>[^)]+)\))?(?:!)?:\s+(?P<summary>.+)$"
    )
    for subject in commits:
        match = conventional.match(subject)
        if match is None:
            groups["Changed"].append(subject)
            continue
        kind = match.group("kind")
        heading = "Added" if kind == "feat" else "Fixed" if kind == "fix" else "Changed"
        scope = match.group("scope")
        summary = match.group("summary")
        entry = f"**{scope}:** {summary}" if scope else summary
        groups[heading].append(entry)

    sections: list[str] = []
    for heading in ("Added", "Fixed", "Changed"):
        entries = groups[heading]
        if entries:
            rendered = "\n".join(f"- {entry}" for entry in entries)
            sections.append(f"### {heading}\n\n{rendered}")
    return "\n\n".join(sections)


def update_changelog(old: str, new: str, commits: list[str]) -> None:
    content = read("CHANGELOG.md")
    heading = "## [Unreleased]"
    start = content.find(heading)
    if start < 0:
        fail("CHANGELOG.md has no Unreleased section")
    body_start = start + len(heading)
    next_heading = content.find("\n## [", body_start)
    if next_heading < 0:
        fail("CHANGELOG.md has no previous release section")
    unreleased = content[body_start:next_heading].strip()
    if not unreleased:
        unreleased = changelog_entries(commits)
    date = datetime.datetime.now(datetime.timezone.utc).date().isoformat()
    release_section = f"{heading}\n\n## [{new}] - {date}\n\n{unreleased}\n"
    content = content[:start] + release_section + content[next_heading:]

    old_link = f"[Unreleased]: https://github.com/envoy1084/envshare/compare/v{old}...HEAD"
    new_links = (
        f"[Unreleased]: https://github.com/envoy1084/envshare/compare/v{new}...HEAD\n"
        f"[{new}]: https://github.com/envoy1084/envshare/releases/tag/v{new}"
    )
    if content.count(old_link) != 1:
        fail("CHANGELOG.md has an unexpected Unreleased comparison link")
    write("CHANGELOG.md", content.replace(old_link, new_links))


def update_versions(old: str, new: str, commits: list[str]) -> None:
    cargo = read("Cargo.toml")
    cargo, count = WORKSPACE_VERSION.subn(rf"\g<1>{new}\g<3>", cargo, count=1)
    if count != 1:
        fail("could not update the workspace version")
    for dependency in ("code", "app-core", "crypto", "network", "protocol"):
        pattern = re.compile(
            rf'(^\s*{re.escape(dependency)}\s*=\s*\{{[^\n]*version\s*=\s*")={re.escape(old)}("[^\n]*\}})',
            re.MULTILINE,
        )
        cargo, count = pattern.subn(rf"\g<1>={new}\g<2>", cargo)
        if count != 1:
            fail(f"could not update the {dependency} workspace dependency")
    write("Cargo.toml", cargo)

    replace("crates/cli/Cargo.toml", f'version = "={old}"', f'version = "={new}"')
    replace("crates/testkit/Cargo.toml", f'version = "={old}"', f'version = "={new}"')
    replace("install.sh", f'INSTALLER_VERSION="{old}"', f'INSTALLER_VERSION="{new}"')
    replace("install.ps1", f'else {{ "{old}" }}', f'else {{ "{new}" }}')
    replace("deploy/docker/Dockerfile", f'ARG VERSION="{old}"', f'ARG VERSION="{new}"')
    update_changelog(old, new, commits)


def main() -> None:
    if len(sys.argv) != 2:
        fail("usage: scripts/prepare-release.py <patch|minor|major>")
    bump = sys.argv[1]
    current = current_version()

    if tag_exists(current):
        commits = release_commits(current)
        prepared = bump_version(current, bump)
        update_versions(current, prepared, commits)
    else:
        prepared = current
        if f"## [{prepared}]" not in read("CHANGELOG.md"):
            fail(f"v{prepared} has no tag and no prepared changelog section")

    print(prepared)


if __name__ == "__main__":
    main()
