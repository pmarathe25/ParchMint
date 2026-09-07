# Install ParchMint

Download your computer's package from
[GitHub Releases](https://github.com/pmarathe25/ParchMint/releases).
Choose an installer under **Assets**. The **Source code** downloads are for
building from source.

| Computer | Package suffix |
| --- | --- |
| Windows, Intel or AMD 64-bit | `windows-x86_64.msi` |
| macOS, Apple silicon (M-series) | `macos-aarch64.dmg` |
| Ubuntu 24.04 or compatible Debian-based Linux, Intel or AMD 64-bit | `linux-x86_64.deb` |

The filename includes the version, for example
`ParchMint-0.1.0-windows-x86_64.msi`. Other Linux distributions and architectures,
including Intel Macs, can [build from source](https://github.com/pmarathe25/ParchMint#run-from-source).

## Windows

Double-click the `.msi` file and approve the installation prompt. Open
**ParchMint** from the Start menu. The installer uses **Program Files** and
requires administrator permission. Windows may warn that the installer is
unsigned.

To remove the app, open **Settings → Apps → Installed apps**, find **ParchMint**,
and choose **Uninstall**.

## macOS

Open the `.dmg` file and drag **ParchMint.app** to **Applications**. Eject the disk
image, then open ParchMint from Applications.

Packages are currently unsigned and not notarized. If macOS blocks the app,
follow Apple's [instructions for opening an app from an unidentified developer](https://support.apple.com/en-ie/102445).
To remove ParchMint, move it from Applications to the Trash.

## Linux

From the download folder, use `apt` to install the package and its dependencies.
Substitute your downloaded filename:

```sh
sudo apt install ./ParchMint-0.1.0-linux-x86_64.deb
```

Open **ParchMint** from the application menu, or run `parchmint` in a terminal.
To remove the app, run `sudo apt remove parchmint`.

If your distribution cannot satisfy the package's runtime dependencies, build
from source on that distribution.

## Update

Close ParchMint and install the newer package using the same steps. On macOS,
choose **Replace** when copying the new app to Applications. Keep projects in
your own folders; updating or uninstalling the app leaves them in place. Open
an existing project folder from the launcher after updating.

## Check a download

Each package has a matching `.sha256` file. Compare its recorded hash with
`Get-FileHash -Algorithm SHA256 <package>` in PowerShell or
`shasum -a 256 <package>` on macOS. On Linux, put both downloads in the same folder
and run `sha256sum --check <package>.sha256`. Replace `<package>` with the
downloaded filename.
