#!/usr/bin/env bash
# Build a single-version APT archive from a verified release package.
set -euo pipefail

: "${APT_SIGNING_KEY:?APT_SIGNING_KEY is required}"
: "${APT_SIGNING_PASSPHRASE:?APT_SIGNING_PASSPHRASE is required}"
: "${APT_SIGNING_FINGERPRINT:?APT_SIGNING_FINGERPRINT is required}"
[[ "$APT_SIGNING_FINGERPRINT" =~ ^[A-Fa-f0-9]{40}$ ]] || { echo 'Expected a full OpenPGP fingerprint' >&2; exit 1; }

release=$(realpath "$1")
mkdir -p "$2"
site=$(realpath "$2")
export GNUPGHOME
GNUPGHOME=$(mktemp -d)
chmod 700 "$GNUPGHOME"
trap 'gpgconf --kill gpg-agent || true; rm -rf -- "$GNUPGHOME"' EXIT
printf '%s\n' "$APT_SIGNING_KEY" | gpg --batch --import
fingerprint=$(gpg --batch --with-colons --list-secret-keys | awk -F: '$1 == "fpr" {print $10; exit}')
[[ "${fingerprint^^}" == "${APT_SIGNING_FINGERPRINT^^}" ]] || { echo 'Signing key fingerprint does not match' >&2; exit 1; }
[[ $(gpg --batch --with-colons --list-secret-keys | awk -F: '$1 == "sec" {n++} END {print n+0}') == 1 ]] || { echo 'Expected exactly one secret key' >&2; exit 1; }

shopt -s nullglob
packages=("$release"/*-linux-x86_64.deb)
[[ ${#packages[@]} == 1 ]] || { echo 'Expected exactly one Linux package' >&2; exit 1; }
mkdir -p "$site/pool/main" "$site/dists/stable/main/binary-amd64"
cp -- "${packages[0]}" "$site/pool/main/"
(
  cd "$site"
  dpkg-scanpackages --arch amd64 pool /dev/null > dists/stable/main/binary-amd64/Packages
  gzip -n -k dists/stable/main/binary-amd64/Packages
  apt-ftparchive -o APT::FTPArchive::Release::Origin=ParchMint \
    -o APT::FTPArchive::Release::Label=ParchMint \
    -o APT::FTPArchive::Release::Suite=stable \
    -o APT::FTPArchive::Release::Codename=stable \
    -o APT::FTPArchive::Release::Architectures=amd64 \
    -o APT::FTPArchive::Release::Components=main \
    release dists/stable > Release
  mv Release dists/stable/Release
  gpg --batch --yes --pinentry-mode loopback --passphrase-fd 3 --local-user "$fingerprint" \
    --clearsign --output dists/stable/InRelease dists/stable/Release 3<<<"$APT_SIGNING_PASSPHRASE"
  gpg --batch --yes --pinentry-mode loopback --passphrase-fd 3 --local-user "$fingerprint" \
    --armor --detach-sign --output dists/stable/Release.gpg dists/stable/Release 3<<<"$APT_SIGNING_PASSPHRASE"
  gpg --batch --armor --export "$fingerprint" > parchmint-archive-key.asc
  gpg --batch --verify dists/stable/InRelease
  gpg --batch --verify dists/stable/Release.gpg dists/stable/Release
)
