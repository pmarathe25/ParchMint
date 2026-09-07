import hashlib
from pathlib import Path
import plistlib
import tempfile
import unittest
import zipfile

from package import archive, dependency_notices, stage


class PackageTests(unittest.TestCase):
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

    def test_portable_archives_include_executable_licenses_and_valid_macos_metadata(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "binary"
            binary.write_bytes(b"release executable")
            for platform in ("linux", "macos", "windows"):
                with self.subTest(platform=platform):
                    staged = root / platform
                    stage(binary, staged, platform, "0.1.0")
                    output = root / f"{platform}.zip"
                    archive(staged, output)
                    with zipfile.ZipFile(output) as bundle:
                        names = bundle.namelist()
                        self.assertIn(f"{platform}/LICENSE", names)
                        self.assertIn(f"{platform}/licenses/source-sans-3.txt", names)
                        self.assertFalse(any("target/" in name or "tests/" in name for name in names))
                        if platform == "macos":
                            metadata = plistlib.loads(bundle.read("macos/ParchMint.app/Contents/Info.plist"))
                            self.assertEqual(metadata["CFBundleVersion"], "0.1.0")
                            executable = "macos/ParchMint.app/Contents/MacOS/parchmint"
                        else:
                            executable = f"{platform}/" + ("parchmint.exe" if platform == "windows" else "parchmint")
                        self.assertEqual(bundle.read(executable), binary.read_bytes())
                        self.assertTrue((bundle.getinfo(executable).external_attr >> 16) & 0o111)
                    digest = hashlib.sha256(output.read_bytes()).hexdigest()
                    self.assertEqual(output.with_suffix(".zip.sha256").read_text(), f"{digest}  {output.name}\n")
                    with self.assertRaises(FileExistsError):
                        archive(staged, output)


if __name__ == "__main__":
    unittest.main()
