#!/bin/sh
# Build a local macOS application bundle. No upload, signing identity, or release.
set -eu
cd "$(dirname "$0")/.."
if [ "$(uname -s)" != Darwin ]; then
  echo 'The native desktop bundle currently requires macOS.' >&2
  exit 1
fi
cargo build --locked --release -p scriptaro-desktop
app_bundle="target/Scriptaro.app"
mkdir -p "$app_bundle/Contents/MacOS"
cp target/release/scriptaro-desktop "$app_bundle/Contents/MacOS/Scriptaro"
cat > "$app_bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>dev.scriptaro.desktop</string>
<key>CFBundleName</key><string>Scriptaro</string>
<key>CFBundleExecutable</key><string>Scriptaro</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
codesign --force --sign - "$app_bundle"
printf 'Local application: %s/%s\n' "$PWD" "$app_bundle"
