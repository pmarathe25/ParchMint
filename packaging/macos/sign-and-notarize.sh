#!/bin/sh
set -eu

app=${1:?app bundle required}
dmg=${2:?dmg path required}
: "${PARCHMINT_APPLE_IDENTITY:?injected signing identity required}"
: "${PARCHMINT_NOTARY_PROFILE:?notarytool keychain profile required}"

codesign --force --deep --options runtime \
  --entitlements packaging/macos/entitlements.plist \
  --sign "$PARCHMINT_APPLE_IDENTITY" "$app"
codesign --verify --deep --strict --verbose=2 "$app"
xcrun notarytool submit "$dmg" --keychain-profile "$PARCHMINT_NOTARY_PROFILE" --wait
xcrun stapler staple "$dmg"
xcrun stapler validate "$dmg"
