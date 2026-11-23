#!/bin/sh
# Build SixNG.app (GUI + CLI) for macOS:
#   scripts/macos-app.sh [--universal] [--prebuilt] [out-dir, default dist]
# --universal builds Apple Silicon and Intel and joins them with lipo; it needs
# both targets (rustup target add aarch64-apple-darwin x86_64-apple-darwin).
# --prebuilt packages existing release binaries instead of building them.
# Needs Rust and CMake (brew install cmake). Ad-hoc signed; not notarised.
set -eu
cd "$(dirname "$0")/.."
universal=0
prebuilt=0
if [ "${1:-}" = "--universal" ]; then universal=1; shift; fi
if [ "${1:-}" = "--prebuilt" ]; then prebuilt=1; shift; fi
out="${1:-dist}"
target="${CARGO_TARGET_DIR:-target}"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
app="$out/SixNG.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
if [ $universal = 1 ]; then
  if [ $prebuilt = 0 ]; then
    for t in aarch64-apple-darwin x86_64-apple-darwin; do
      MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --release --target $t
    done
  fi
  for b in sixng-gui sixng; do
    lipo -create -output "$app/Contents/MacOS/$b" \
      "$target/aarch64-apple-darwin/release/$b" "$target/x86_64-apple-darwin/release/$b"
  done
else
  if [ $prebuilt = 0 ]; then
    MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --release
  fi
  cp "$target/release/sixng-gui" "$target/release/sixng" "$app/Contents/MacOS/"
fi

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
  <key>LSSupportsGameMode</key><true/>
  <key>GCSupportsControllerUserInteraction</key><true/>
</dict></plist>
PLIST
codesign --force --deep --sign - "$app"
echo "$app"
