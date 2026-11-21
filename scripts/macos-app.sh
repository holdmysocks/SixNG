#!/bin/sh
# Build SixNG.app (GUI + CLI) for macOS: scripts/macos-app.sh [out-dir, default dist]
# Needs Rust and CMake (brew install cmake). Ad-hoc signed; not notarised.
set -eu
cd "$(dirname "$0")/.."
out="${1:-dist}"
target="${CARGO_TARGET_DIR:-target}"
cargo build --release
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
app="$out/SixNG.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$target/release/sixng-gui" "$target/release/sixng" "$app/Contents/MacOS/"

# icon from the 512 px logo
iconset=$(mktemp -d)/sixng.iconset
mkdir -p "$iconset"
for s in 16 32 128 256 512; do
  sips -z $s $s assets/logo/sixng_icon_512.png --out "$iconset/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2)); [ $d -le 512 ] && sips -z $d $d assets/logo/sixng_icon_512.png --out "$iconset/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/sixng.icns"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>SixNG</string>
  <key>CFBundleDisplayName</key><string>SixNG</string>
  <key>CFBundleIdentifier</key><string>io.github.sixng</string>
  <key>CFBundleExecutable</key><string>sixng-gui</string>
  <key>CFBundleIconFile</key><string>sixng</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
codesign --force --deep --sign - "$app"
echo "$app"
