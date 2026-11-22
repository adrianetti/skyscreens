#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
(cd .. && PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH" cargo build --release -p link)
swift build -c release
APP=.build/Skyscreens.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp .build/release/skyscreens .build/release/skyscreens-displays "$APP/Contents/MacOS/"
cat > "$APP/Contents/Info.plist" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>io.github.adrianetti.skyscreens</string>
  <key>CFBundleName</key><string>Skyscreens</string>
  <key>CFBundleExecutable</key><string>skyscreens</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>LSUIElement</key><true/>
</dict>
</plist>
PLIST
identity() { security find-identity -p codesigning 2> /dev/null | awk '/Apple Development|Skyscreens/ && !n++ { print $2 }' || true; }
SIGN="${SKYSCREENS_SIGN_ID:-$(identity)}"
if [[ -z "$SIGN" ]]; then
  SIGN=-
  echo "warning: ad-hoc signature, so macOS will ask for Screen Recording again after every rebuild." >&2
  echo "         Create a signing identity once: docs/development.md#code-signing" >&2
fi
codesign --force --sign "$SIGN" "$APP/Contents/MacOS/skyscreens-displays"
codesign --force --sign "$SIGN" "$APP"
echo "$PWD/$APP"
