import hashlib
import json
import subprocess
import sys
import shutil
from pathlib import Path
import plistlib
import tempfile
import unittest
from unittest import mock

from package import build_package, cargo_metadata, dependency_notices, native_version, stage


class PackageTests(unittest.TestCase):
    def test_cargo_metadata_uses_utf8_even_with_a_windows_code_page(self):
        metadata = {"packages": [{"description": "はと — a writing tool"}], "workspace_members": []}
        payload = json.dumps(metadata, ensure_ascii=False).encode("utf-8")
        real_run = subprocess.run

        def emit_metadata(_command, **options):
            return real_run(
                [sys.executable, "-c", f"import os; os.write(1, bytes.fromhex('{payload.hex()}'))"],
                **options,
            )

        # Exercise Python's actual pipe decoder, forcing the Windows fallback
        # even when this directed regression runs on Linux or macOS.
        with mock.patch("subprocess._text_encoding", return_value="cp1252"), \
                mock.patch("package.subprocess.run", side_effect=emit_metadata):
            self.assertEqual(cargo_metadata(), metadata)

    def test_dependency_notices_preserve_source_text_and_version_metadata(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "dependency"
            source.mkdir()
            (source / "LICENSE-MIT").write_text("upstream copyright and license")
            destination = root / "release"
            dependency_notices(destination, {
                "workspace_members": [],
                "packages": [{"id": "example", "name": "example", "version": "1.2.3",
                              "license": "MIT", "repository": "https://example.com/source",
                              "manifest_path": str(source / "Cargo.toml"), "license_file": None}],
            })
            notices = destination / "licenses/dependencies"
            self.assertEqual((notices / "example-1.2.3/LICENSE-MIT").read_text(),
                             "upstream copyright and license")
            self.assertIn('"version": "1.2.3"', (notices / "packages.json").read_text())
            self.assertTrue((notices / "Cargo.lock").is_file())

    def test_staging_keeps_executable_and_notices_in_the_installed_application(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "binary"
            binary.write_bytes(b"release executable")
            layouts = {
                "linux": ("usr/bin/parchmint", "usr/share/doc/parchmint"),
                "macos": ("ParchMint.app/Contents/MacOS/parchmint", "ParchMint.app/Contents/Resources"),
                "windows": ("parchmint.exe", "."),
            }
            for platform, (executable, documents) in layouts.items():
                with self.subTest(platform=platform):
                    staged = root / platform
                    documentation = stage(binary, staged, platform, "0.1.0")
                    self.assertEqual(documentation, staged / documents)
                    for name in ("LICENSE", "install.md", "user-guide.md", "licenses/source-sans-3.txt",
                                 "licenses/source-serif-4.txt"):
                        self.assertTrue((documentation / name).is_file(), name)
                    self.assertEqual((staged / executable).read_bytes(), binary.read_bytes())
                    if sys.platform != "win32":
                        self.assertEqual((staged / executable).stat().st_mode & 0o777, 0o755)
                    if platform == "macos":
                        metadata = plistlib.loads((staged / "ParchMint.app/Contents/Info.plist").read_bytes())
                        self.assertEqual(metadata["CFBundleVersion"], "0.1.0")
                        self.assertEqual(metadata["CFBundleExecutable"], "parchmint")

    def test_native_versions_do_not_silently_change_upgrade_ordering(self):
        self.assertEqual(native_version("0.1.0"), "0.1.0")
        for version in ("0.1.0-rc.1", "0.1.0+build", "01.2.3", "256.0.0", "1.256.0", "1.0.65536"):
            with self.subTest(version=version), self.assertRaises(ValueError):
                native_version(version)

    @unittest.skipUnless(sys.platform == "linux" and shutil.which("dpkg-shlibdeps"), "requires Debian packaging tools")
    def test_debian_package_installs_menu_entry_dependencies_and_executable(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            staged = root / "payload"
            binary = Path("/bin/true")  # Small real ELF file exercises dependency discovery.
            stage(binary, staged, "linux", "0.1.0")
            output = root / "ParchMint.deb"
            build_package(staged, output, "linux", "0.1.0", "x86_64")
            control = subprocess.check_output(["dpkg-deb", "--field", str(output)], encoding="utf-8")
            for field in ("Package: parchmint", "Version: 0.1.0", "Architecture: amd64", "libc6 (>=", "libxkbcommon-x11-0"):
                self.assertIn(field, control)
            installed = root / "installed"
            subprocess.run(["dpkg-deb", "--extract", str(output), str(installed)], check=True)
            executable = installed / "usr/bin/parchmint"
            self.assertEqual(executable.read_bytes(), binary.read_bytes())
            self.assertEqual(executable.stat().st_mode & 0o777, 0o755)
            self.assertIn("Exec=parchmint %f", (installed / "usr/share/applications/parchmint.desktop").read_text())
            self.assertTrue((installed / "usr/share/doc/parchmint/LICENSE").is_file())
            digest = hashlib.sha256(output.read_bytes()).hexdigest()
            self.assertEqual(output.with_suffix(".deb.sha256").read_text(), f"{digest}  {output.name}\n")
            with self.assertRaises(FileExistsError):
                build_package(staged, output, "linux", "0.1.0", "x86_64")


if __name__ == "__main__":
    unittest.main()
