"""Validate an existing GitHub release before treating publication as complete."""

import argparse
import json
import re
import sys


def expected_assets(tag):
    """Return the package and checksum names required for a release tag."""
    if not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag):
        raise ValueError(f"release tag is not a numeric version: {tag}")
    version = tag.removeprefix("v")
    packages = (
        f"ParchMint-{version}-linux-x86_64.deb",
        f"ParchMint-{version}-macos-aarch64.dmg",
        f"ParchMint-{version}-windows-x86_64.msi",
    )
    return {*packages, *(f"{package}.sha256" for package in packages)}


def verify_existing_release(release, tag, commit):
    """Reject an existing release that does not match this publication."""
    problems = []
    if release.get("tagName") != tag:
        problems.append(f"tag is {release.get('tagName')!r}, expected {tag!r}")
    if release.get("targetCommitish") != commit:
        problems.append(
            f"target commit is {release.get('targetCommitish')!r}, expected {commit!r}"
        )
    assets = {
        asset.get("name")
        for asset in release.get("assets", [])
        if isinstance(asset, dict) and isinstance(asset.get("name"), str)
    }
    missing = sorted(expected_assets(tag) - assets)
    if missing:
        problems.append(f"missing assets: {', '.join(missing)}")
    if problems:
        raise ValueError("; ".join(problems))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--commit", required=True)
    args = parser.parse_args()
    try:
        verify_existing_release(json.load(sys.stdin), args.tag, args.commit)
    except ValueError as error:
        raise SystemExit(f"Existing release does not match this build: {error}") from error


if __name__ == "__main__":
    main()
