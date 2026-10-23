#!/usr/bin/env python3
"""Set the crate version, commit it, and create its matching release tag."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SEMVER = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\Z")


def run(*args: str, capture: bool = False) -> str:
    result = subprocess.run(
        args,
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
    )
    return result.stdout.strip() if capture else ""


def main() -> int:
    if len(sys.argv) != 2:
        print("Usage: scripts/release.py <version> (for example, 0.2.0)", file=sys.stderr)
        return 2

    version = sys.argv[1].removeprefix("v")
    if not SEMVER.fullmatch(version):
        print("Version must be stable MAJOR.MINOR.PATCH semver, such as 1.2.3.", file=sys.stderr)
        return 2

    status = run("git", "status", "--porcelain", capture=True)
    if status:
        print("Working tree must be clean before preparing a release:", file=sys.stderr)
        print(status, file=sys.stderr)
        return 1

    tag = f"v{version}"
    existing_tag = subprocess.run(
        ["git", "show-ref", "--verify", "--quiet", f"refs/tags/{tag}"],
        cwd=ROOT,
        check=False,
    )
    if existing_tag.returncode == 0:
        print(f"Tag {tag} already exists.", file=sys.stderr)
        return 1

    manifest_path = ROOT / "Cargo.toml"
    manifest = manifest_path.read_text()
    manifest_version = re.compile(r'(?m)^version = "([^"\n]+)"$')
    if not manifest_version.search(manifest):
        print("Could not find the package version in Cargo.toml.", file=sys.stderr)
        return 1
    updated_manifest = manifest_version.sub(f'version = "{version}"', manifest, count=1)

    lock_path = ROOT / "Cargo.lock"
    lock = lock_path.read_text()
    package_blocks = re.split(r"(?=^\[\[package\]\]$)", lock, flags=re.MULTILINE)
    updated_blocks: list[str] = []
    found_package = False
    for block in package_blocks:
        if re.search(r'(?m)^name = "button-studio"$', block):
            block, replacements = re.subn(
                r'(?m)^version = "[^"\n]+"$',
                f'version = "{version}"',
                block,
                count=1,
            )
            if replacements != 1:
                print('Could not update button-studio in Cargo.lock.', file=sys.stderr)
                return 1
            found_package = True
        updated_blocks.append(block)
    if not found_package:
        print('Could not find button-studio in Cargo.lock.', file=sys.stderr)
        return 1

    manifest_path.write_text(updated_manifest)
    lock_path.write_text("".join(updated_blocks))

    try:
        metadata = run("cargo", "metadata", "--locked", "--no-deps", "--format-version", "1", capture=True)
        package = next(
            package
            for package in json.loads(metadata)["packages"]
            if package["name"] == "button-studio"
        )
        if package["version"] != version:
            raise RuntimeError(f"Cargo reports version {package['version']}, expected {version}.")
        run("git", "diff", "--check")
        run("git", "add", "Cargo.toml", "Cargo.lock")
        run("git", "commit", "-m", f"Release {tag}")
        run("git", "tag", "-a", tag, "-m", f"Release {tag}")
    except (subprocess.CalledProcessError, RuntimeError, StopIteration) as error:
        print(f"Release preparation failed: {error}", file=sys.stderr)
        print("The Cargo version files may have been updated; review the working tree before retrying.", file=sys.stderr)
        return 1

    branch = run("git", "branch", "--show-current", capture=True)
    if branch:
        print(f"Created commit and tag {tag}. Publish them with: git push origin {branch} --follow-tags")
    else:
        print(f"Created commit and tag {tag}. Push the commit and tag to your remote to start the release workflow.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
