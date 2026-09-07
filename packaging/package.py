"""Stage portable release archives from an already built desktop executable."""

import argparse
import hashlib
import json
from pathlib import Path
import plistlib
import platform as host_platform
import shutil
import subprocess
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent.parent


def stage(binary, destination, platform, version):
    """Copy runtime files and notices into a new archive root."""
    destination.mkdir()
    if platform == "macos":
        contents = destination / "ParchMint.app" / "Contents"
        executable = contents / "MacOS" / "parchmint"
        executable.parent.mkdir(parents=True)
        with (contents / "Info.plist").open("wb") as output:
            plistlib.dump({
                "CFBundleIdentifier": "com.parchmint.desktop",
                "CFBundleName": "ParchMint",
                "CFBundleDisplayName": "ParchMint",
                "CFBundleExecutable": "parchmint",
                "CFBundlePackageType": "APPL",
                "CFBundleShortVersionString": version,
                "CFBundleVersion": version,
                "NSHighResolutionCapable": True,
            }, output)
    else:
        executable = destination / ("parchmint.exe" if platform == "windows" else "parchmint")
    shutil.copy2(binary, executable)
    executable.chmod(0o755)
    shutil.copy2(ROOT / "LICENSE", destination / "LICENSE")
    shutil.copy2(ROOT / "docs/user-guide.md", destination / "user-guide.md")
    notices = destination / "licenses"
    notices.mkdir()
    fonts = ROOT / "crates/parchmint-ui-iced/assets/fonts"
    for family in ("source-sans-3", "source-serif-4"):
        shutil.copy2(fonts / family / "LICENSE.md", notices / f"{family}.txt")
    if platform == "linux":
        shutil.copy2(ROOT / "packaging/linux/parchmint.desktop", destination / "parchmint.desktop")
    (destination / "README.txt").write_text(
        f"ParchMint {version}\n\n"
        "Extract the whole archive before launching ParchMint.\n"
        "On macOS, open ParchMint.app. On Windows, run parchmint.exe.\n"
        "On Linux, run ./parchmint; installing the desktop entry also requires\n"
        "placing the executable on PATH.\n\n"
        "Projects stay in the folders you choose. See user-guide.md for usage.\n"
        "Source and dependency versions: https://github.com/pmarathe25/ParchMint\n"
        "Use the source revision accompanying this release.\n",
        encoding="utf-8",
    )


def dependency_notices(destination, metadata):
    """Record locked dependencies and preserve notices shipped in their sources."""
    notices = destination / "licenses" / "dependencies"
    notices.mkdir(parents=True)
    members = set(metadata["workspace_members"])
    inventory = []
    for package in sorted(metadata["packages"], key=lambda item: (item["name"], item["version"])):
        if package["id"] in members:
            continue
        name = f'{package["name"]}-{package["version"]}'
        inventory.append({key: package[key] for key in ("name", "version", "license", "repository")})
        root = Path(package["manifest_path"]).parent
        files = {path for path in root.iterdir()
                 if path.is_file() and path.name.lower().startswith(("license", "copying", "notice", "unlicense"))}
        if package.get("license_file"):
            files.add(root / package["license_file"])
        if files:
            package_notices = notices / name
            package_notices.mkdir()
            for path in sorted(files):
                shutil.copy2(path, package_notices / path.name)
    (notices / "packages.json").write_text(json.dumps(inventory, indent=2) + "\n", encoding="utf-8")
    shutil.copy2(ROOT / "Cargo.lock", notices / "Cargo.lock")


def archive(staged, output):
    """Write a new ZIP with executable permissions and a SHA-256 sidecar."""
    checksum = output.with_suffix(output.suffix + ".sha256")
    if output.exists() or checksum.exists():
        raise FileExistsError(f"release output already exists: {output}")
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, "x", compression=zipfile.ZIP_DEFLATED) as bundle:
        for path in sorted(staged.rglob("*")):
            if path.is_file():
                info = zipfile.ZipInfo.from_file(path, path.relative_to(staged.parent), strict_timestamps=False)
                # Windows stat results do not preserve executable Unix mode bits.
                # Normalize archive modes so every platform produces usable bundles.
                mode = 0o755 if path.name in ("parchmint", "parchmint.exe") else 0o644
                info.create_system = 3
                info.external_attr = (0o100000 | mode) << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                with path.open("rb") as source, bundle.open(info, "w") as target:
                    shutil.copyfileobj(source, target)
    with output.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    checksum.write_text(f"{digest}  {output.name}\n", encoding="ascii")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--platform", choices=("linux", "macos", "windows"), required=True)
    parser.add_argument("--architecture", choices=("x86_64", "aarch64"),
                        default={"amd64": "x86_64", "arm64": "aarch64"}.get(host_platform.machine().lower(), host_platform.machine().lower()))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    binary = args.binary.resolve(strict=True)
    # A desktop-only release build avoids workspace feature unification with tests.
    result = subprocess.run([str(binary), "--version"], check=True, capture_output=True, text=True)
    if result.stdout.strip() != f"ParchMint {version}":
        raise ValueError("executable version does not match Cargo.toml")
    metadata = json.loads(subprocess.run(
        ["cargo", "metadata", "--locked", "--offline", "--format-version", "1"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout)
    name = f"ParchMint-{version}-{args.platform}-{args.architecture}"
    with tempfile.TemporaryDirectory(prefix="parchmint-package-") as temporary:
        staged = Path(temporary) / name
        stage(binary, staged, args.platform, version)
        revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
        dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True))
        (staged / "source.json").write_text(
            json.dumps({"revision": revision, "dirty": dirty, "version": version}, indent=2) + "\n",
            encoding="utf-8",
        )
        dependency_notices(staged, metadata)
        archive(staged, args.output / f"{name}.zip")


if __name__ == "__main__":
    main()
