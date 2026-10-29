#!/bin/zsh
# Builds Skyscreens.app, so macOS grants Screen Recording to skyscreens itself
# rather than to the terminal that launched it.
# Set SKYSCREENS_SIGN_ID to a code signing identity to keep the permission across rebuilds;
# with the default ad-hoc signature macOS may ask again after each rebuild.
set -euo pipefail
cd "$(dirname "$0")/.."
swift build -c release
APP=.build/Skyscreens.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp .build/release/skyscreens .build/release/skyscreens-displays "$APP/Contents/MacOS/"
cat > "$APP/Contents/Info.plist" <<PLIST
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
SIGN="${SKYSCREENS_SIGN_ID:--}"
codesign --force --sign "$SIGN" "$APP/Contents/MacOS/skyscreens-displays"
codesign --force --sign "$SIGN" "$APP"
echo "$PWD/$APP"
