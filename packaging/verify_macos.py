"""Install a DMG into a disposable directory and exercise its native application.

Run on a disposable macOS runner/profile: the application uses normal OS paths
for preferences, while the project and installed copy live under a temp folder.
"""

import argparse
import os
from pathlib import Path
import plistlib
import shutil
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def run(command):
    print("+", " ".join(map(str, command)), flush=True)
    return subprocess.run(list(map(str, command)), check=True, capture_output=True,
                          encoding="utf-8", timeout=180).stdout


def verify(image, output, architecture, version):
    image = image.resolve(strict=True)
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    run(["hdiutil", "verify", image])
    with tempfile.TemporaryDirectory(prefix="parchmint-installed-") as temporary:
        root = Path(temporary).resolve()
        mount = root / "volume"
        mount.mkdir()
        installed = root / "Applications" / "ParchMint.app"
        installed.parent.mkdir()
        run(["hdiutil", "attach", image, "-mountpoint", mount, "-readonly", "-nobrowse"])
        try:
            run(["codesign", "--verify", "--deep", "--strict", "--verbose=2", mount / "ParchMint.app"])
            run(["ditto", mount / "ParchMint.app", installed])
        finally:
            run(["hdiutil", "detach", mount])

        # All subsequent checks use the copied application, with the DMG ejected.
        run(["codesign", "--verify", "--deep", "--strict", "--verbose=2", installed])
        contents = installed / "Contents"
        metadata = plistlib.loads((contents / "Info.plist").read_bytes())
        if metadata["CFBundleVersion"] != version:
            raise ValueError("installed application has the wrong version")
        for resource in (metadata["CFBundleIconFile"], "licenses/dependencies/packages.json"):
            if not (contents / "Resources" / resource).is_file():
                raise ValueError(f"missing installed resource: {resource}")
        binary = contents / "MacOS" / metadata["CFBundleExecutable"]
        if run([binary, "--version"]).strip() != f"ParchMint {version}":
            raise ValueError("installed executable has the wrong version")
        if run(["lipo", "-archs", binary]).strip() != architecture:
            raise ValueError("installed executable has the wrong architecture")
        dependencies = run(["otool", "-L", binary])
        (output / "dependencies.txt").write_text(dependencies, encoding="utf-8")
        for line in dependencies.splitlines()[1:]:
            dependency = line.strip().split(" (", 1)[0]
            if not dependency.startswith(("/usr/lib/", "/System/Library/")):
                raise ValueError(f"non-system dependency would prevent installation: {dependency}")
        if os.environ.get("PARCHMINT_MACOS_NOTARY_PROFILE"):
            run(["xcrun", "stapler", "validate", image])
            run(["spctl", "--assess", "--type", "execute", "--verbose=2", installed])

        project = root / "Smoke test.parchmint"
        shutil.copytree(ROOT / "tests/parchmint-test-support/fixtures/canonical/minimal-project", project)
        for target, appearance, finder_launch in [("editor", "light", False),
                                                  ("editor", "dark", True),
                                                  ("cards", "light", False)]:
            screenshot = output / f"{target}-{appearance}.png"
            arguments = ["capture", "--project", project, "--target", target,
                         "--appearance", appearance, "--output", screenshot,
                         "--scale", "1", "--logical-width", "1280", "--logical-height", "720"]
            command = (["open", "-W", "-n", "-a", installed, "--args"]
                       if finder_launch else [binary])
            log = run([*command, *arguments])
            (output / f"{target}-{appearance}.log").write_text(log, encoding="utf-8")
            # Capture exits only after the production editor mounts and draws.
            # LaunchServices does not propagate its child's exit code, so output
            # verification is mandatory even when `open` succeeds.
            pixels = screenshot.read_bytes()
            if pixels[:8] != b"\x89PNG\r\n\x1a\n" or len(pixels) < 4096:
                raise ValueError(f"native application did not produce a complete capture: {screenshot}")
            width, height = struct.unpack(">II", pixels[16:24])
            if width < 1280 or height < 720:
                raise ValueError(f"native capture is too small: {width}x{height}")
        (output / "verified.txt").write_text(
            f"ParchMint {version} ({architecture})\nDMG verified, copied, and ejected.\n"
            "Installed signature, resources, dependencies, editor and Overview passed.\n"
            "Dark editor launched through macOS LaunchServices.\n", encoding="utf-8")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--architecture", choices=("arm64", "x86_64"), required=True)
    parser.add_argument("--version", required=True)
    args = parser.parse_args()
    try:
        verify(args.image, args.output, args.architecture, args.version)
    except subprocess.CalledProcessError as error:
        print(error.stdout or "", flush=True)
        print(error.stderr or "", flush=True)
        raise
