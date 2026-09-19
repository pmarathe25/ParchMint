"""Import release signing credentials into an ephemeral CI keychain."""
import base64
import os
from pathlib import Path
import secrets
import subprocess


def configure():
    required = ("CERTIFICATE", "CERTIFICATE_PASSWORD", "SIGNING_IDENTITY",
                "APPLE_ID", "APPLE_PASSWORD", "TEAM_ID")
    missing = [name for name in required if not os.environ.get(name)]
    if len(missing) == len(required):
        print("No Apple credentials configured; building with an ad hoc signature (not notarized).")
        return
    if missing:
        raise SystemExit("Incomplete macOS signing configuration; missing: " + ", ".join(missing))
    temporary = Path(os.environ["RUNNER_TEMP"])
    keychain = temporary / "parchmint-signing.keychain-db"
    certificate = temporary / "parchmint-signing.p12"
    password = secrets.token_urlsafe(32)
    certificate.write_bytes(base64.b64decode(os.environ["CERTIFICATE"], validate=True))
    certificate.chmod(0o600)
    try:
        subprocess.run(["security", "create-keychain", "-p", password, str(keychain)], check=True)
        subprocess.run(["security", "set-keychain-settings", "-lut", "21600", str(keychain)], check=True)
        subprocess.run(["security", "unlock-keychain", "-p", password, str(keychain)], check=True)
        subprocess.run(["security", "import", str(certificate), "-P", os.environ["CERTIFICATE_PASSWORD"],
                        "-k", str(keychain), "-T", "/usr/bin/codesign"], check=True)
        subprocess.run(["security", "set-key-partition-list", "-S", "apple-tool:,apple:,codesign:",
                        "-s", "-k", password, str(keychain)], check=True, stdout=subprocess.DEVNULL)
        subprocess.run(["security", "list-keychains", "-d", "user", "-s", str(keychain)], check=True)
        subprocess.run(["xcrun", "notarytool", "store-credentials", "parchmint-release",
                        "--apple-id", os.environ["APPLE_ID"], "--team-id", os.environ["TEAM_ID"],
                        "--password", os.environ["APPLE_PASSWORD"]], check=True)
        with Path(os.environ["GITHUB_ENV"]).open("a", encoding="utf-8") as output:
            output.write(f"PARCHMINT_MACOS_SIGNING_IDENTITY={os.environ['SIGNING_IDENTITY']}\n")
            output.write("PARCHMINT_MACOS_NOTARY_PROFILE=parchmint-release\nPARCHMINT_REQUIRE_NOTARIZATION=1\n")
    finally:
        certificate.unlink(missing_ok=True)


if __name__ == "__main__":
    configure()
