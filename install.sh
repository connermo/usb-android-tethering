#!/bin/sh
# usb-android-tethering installer.
#
#   Install:    curl -fsSL https://raw.githubusercontent.com/frosty-universe/usb-android-tethering/main/install.sh | sh
#   Uninstall:  curl -fsSL https://raw.githubusercontent.com/frosty-universe/usb-android-tethering/main/install.sh | sh -s -- --uninstall
#
# It downloads the latest release (universal binary: Apple Silicon + Intel), verifies its
# SHA-256 checksum against the release's SHA256SUMS, and runs `sudo usb-android-tethering
# install`, which sets up the background service. Nothing is installed if any step fails.
#
# Environment: UAT_VERSION=v0.1.0 installs a specific release instead of the latest.
set -eu

REPO="frosty-universe/usb-android-tethering"
ASSET="usb-android-tethering-macos-universal.tar.gz"
BIN="/usr/local/bin/usb-android-tethering"

say()  { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || fail "this tool is for macOS only"
for tool in curl shasum tar sudo; do
  command -v "$tool" >/dev/null 2>&1 || fail "'$tool' is required but was not found"
done

if [ "${1:-}" = "--uninstall" ]; then
  [ -x "$BIN" ] || fail "usb-android-tethering is not installed"
  say "Removing usb-android-tethering (you may be asked for your password)…"
  sudo "$BIN" uninstall --purge
  say "Uninstalled. Your network settings are back to how they were."
  exit 0
fi

if [ -n "${UAT_VERSION:-}" ]; then
  base="https://github.com/$REPO/releases/download/$UAT_VERSION"
else
  base="https://github.com/$REPO/releases/latest/download"
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "Downloading usb-android-tethering ${UAT_VERSION:-(latest)}…"
curl -fsSL --retry 3 -o "$tmp/$ASSET" "$base/$ASSET" || fail "download failed: $base/$ASSET"
curl -fsSL --retry 3 -o "$tmp/SHA256SUMS" "$base/SHA256SUMS" || fail "checksum download failed"

say "Verifying checksum…"
( cd "$tmp" && grep " $ASSET\$" SHA256SUMS | shasum -a 256 -c - >/dev/null ) \
  || fail "checksum mismatch — refusing to install"

tar -xzf "$tmp/$ASSET" -C "$tmp" usb-android-tethering
chmod 755 "$tmp/usb-android-tethering"
"$tmp/usb-android-tethering" --version >/dev/null 2>&1 || fail "downloaded binary does not run on this Mac"

say "Installing the background service (you may be asked for your password)…"
sudo "$tmp/usb-android-tethering" install

cat <<'EOF'

  ✔ Installed. Next:
    1. Connect your Android phone with a USB data cable.
    2. On the phone, turn on USB tethering
       (Settings → search for "USB tethering").
    3. Check the connection:   usb-android-tethering status

  Uninstall any time:
    curl -fsSL https://raw.githubusercontent.com/frosty-universe/usb-android-tethering/main/install.sh | sh -s -- --uninstall

EOF
