"""Build native release packages from an already built desktop executable."""

import argparse
import hashlib
import json
from pathlib import Path
import plistlib
import platform as host_platform
import re
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent


def stage(binary, destination, platform, version):
    """Stage installed files and return the directory for documentation/notices."""
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
        documentation = contents / "Resources"
    elif platform == "linux":
        executable = destination / "usr/bin/parchmint"
        documentation = destination / "usr/share/doc/parchmint"
        applications = destination / "usr/share/applications"
        applications.mkdir(parents=True)
        shutil.copy2(ROOT / "packaging/linux/parchmint.desktop", applications / "parchmint.desktop")
    else:
        executable = destination / "parchmint.exe"
        documentation = destination
    executable.parent.mkdir(parents=True, exist_ok=True)
    documentation.mkdir(parents=True, exist_ok=True)
    shutil.copy2(binary, executable)
    executable.chmod(0o755)
    shutil.copy2(ROOT / "LICENSE", documentation / "LICENSE")
    shutil.copy2(ROOT / "docs/install.md", documentation / "install.md")
    shutil.copy2(ROOT / "docs/user-guide.md", documentation / "user-guide.md")
    notices = documentation / "licenses"
    notices.mkdir()
    fonts = ROOT / "crates/parchmint-ui-iced/assets/fonts"
    for family in ("source-sans-3", "source-serif-4"):
        shutil.copy2(fonts / family / "LICENSE.md", notices / f"{family}.txt")
    return documentation


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


def native_version(version):
    # Keep one numeric version contract across MSI, Apple bundles, and Debian.
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version):
        raise ValueError("native releases require a numeric major.minor.patch version")
    if any(int(part) > limit for part, limit in zip(version.split("."), (255, 255, 65535))):
        raise ValueError("native release version exceeds Windows Installer limits")
    return version


def debian_package(staged, output, version, architecture):
    # dpkg-shlibdeps needs source metadata, kept outside the installed payload.
    with tempfile.TemporaryDirectory(prefix="parchmint-shlibs-") as temporary:
        source = Path(temporary) / "debian"
        source.mkdir()
        (source / "control").write_text("Source: parchmint\n\nPackage: parchmint\nArchitecture: any\n", encoding="utf-8")
        result = subprocess.run(
            ["dpkg-shlibdeps", "-O", "-e" + str(staged / "usr/bin/parchmint")],
            cwd=temporary, check=True, stdout=subprocess.PIPE, encoding="utf-8",
        )
    linked = next(line.removeprefix("shlibs:Depends=") for line in result.stdout.splitlines()
                  if line.startswith("shlibs:Depends="))
    # Winit loads these at runtime, so they do not appear in ELF dependencies.
    dependencies = linked + ", libx11-6, libx11-xcb1, libxcursor1, libxi6, libxkbcommon-x11-0, libwayland-client0"
    control = staged / "DEBIAN"
    control.mkdir()
    size = sum(path.stat().st_size for path in staged.rglob("*") if path.is_file())
    (control / "control").write_text(
        f"Package: parchmint\nVersion: {version}\nArchitecture: {architecture}\n"
        "Maintainer: ParchMint contributors <pmarathe25@users.noreply.github.com>\n"
        "Section: editors\nPriority: optional\n"
        f"Depends: {dependencies}\nInstalled-Size: {(size + 1023) // 1024}\n"
        "Homepage: https://github.com/pmarathe25/ParchMint\n"
        "Description: Write and organize novels locally\n"
        " A desktop writing workspace with local projects and save history.\n",
        encoding="utf-8",
    )
    subprocess.run(["dpkg-deb", "--build", "--root-owner-group", "-Zxz", "--threads-max=1",
                    str(staged), str(output)], check=True)


def build_package(staged, output, platform, version, architecture):
    """Build with the host's native tools and hash the finished package."""
    native_version(version)
    output = output.resolve()
    staged = staged.resolve()
    checksum = output.with_suffix(output.suffix + ".sha256")
    if output.exists() or checksum.exists():
        raise FileExistsError(f"release output already exists: {output}")
    output.parent.mkdir(parents=True, exist_ok=True)
    if platform == "linux":
        debian_package(staged, output, version, {"x86_64": "amd64", "aarch64": "arm64"}[architecture])
    elif platform == "macos":
        (staged / "Applications").symlink_to("/Applications", target_is_directory=True)
        subprocess.run(["hdiutil", "create", "-volname", "ParchMint", "-srcfolder", str(staged),
                        "-format", "UDZO", str(output)], check=True)
    elif platform == "windows":
        subprocess.run(["wix", "build", str(ROOT / "packaging/windows/package.wxs"),
                        "-arch", {"x86_64": "x64", "aarch64": "arm64"}[architecture],
                        "-d", f"Version={version}", "-bindpath", f"payload={staged}",
                        "-intermediatefolder", str(staged.parent / "wix"),
                        "-pdbtype", "none", "-out", str(output)], check=True)
    else:
        raise ValueError(f"unsupported package platform: {platform}")
    with output.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    checksum.write_text(f"{digest}  {output.name}\n", encoding="ascii")


def cargo_metadata():
    """Cargo emits UTF-8 JSON independently of the host's console code page."""
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--offline", "--format-version", "1"],
        cwd=ROOT, check=True, capture_output=True, encoding="utf-8",
    )
    return json.loads(result.stdout)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--platform", choices=("linux", "macos", "windows"), required=True)
    parser.add_argument("--architecture", choices=("x86_64", "aarch64"),
                        default={"amd64": "x86_64", "arm64": "aarch64"}.get(host_platform.machine().lower(), host_platform.machine().lower()))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    native_version(version)
    binary = args.binary.resolve(strict=True)
    # A desktop-only release build avoids workspace feature unification with tests.
    result = subprocess.run([str(binary), "--version"], check=True, capture_output=True, encoding="utf-8")
    if result.stdout.strip() != f"ParchMint {version}":
        raise ValueError("executable version does not match Cargo.toml")
    metadata = cargo_metadata()
    name = f"ParchMint-{version}-{args.platform}-{args.architecture}"
    with tempfile.TemporaryDirectory(prefix="parchmint-package-") as temporary:
        staged = Path(temporary) / name
        documentation = stage(binary, staged, args.platform, version)
        revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, encoding="utf-8").strip()
        dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, encoding="utf-8"))
        (documentation / "source.json").write_text(
            json.dumps({"revision": revision, "dirty": dirty, "version": version}, indent=2) + "\n",
            encoding="utf-8",
        )
        dependency_notices(documentation, metadata)
        extension = {"linux": "deb", "macos": "dmg", "windows": "msi"}[args.platform]
        build_package(staged, args.output / f"{name}.{extension}", args.platform, version, args.architecture)


if __name__ == "__main__":
    main()
