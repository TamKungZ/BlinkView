#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

NAME=blinkview
APP_NAME=BlinkView
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
ARCH=$(uname -m)
DIST="$ROOT/dist"
BUILD="$ROOT/target/package"
APP="$BUILD/$APP_NAME.app"

cargo build --release
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

install -m 0755 target/release/blinkview "$APP/Contents/MacOS/blinkview"
install -m 0644 assets/blinkview.icns "$APP/Contents/Resources/blinkview.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>$APP_NAME</string>
  <key>CFBundleDisplayName</key>
  <string>$APP_NAME</string>
  <key>CFBundleIdentifier</key>
  <string>me.tamkungz.blinkview</string>
  <key>CFBundleVersion</key>
  <string>$VERSION</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundleExecutable</key>
  <string>blinkview</string>
  <key>CFBundleIconFile</key>
  <string>blinkview</string>
  <key>LSMinimumSystemVersion</key>
  <string>10.13</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
</dict>
</plist>
PLIST

mkdir -p "$DIST"
TAR="$DIST/${NAME}-${VERSION}-macos-${ARCH}.tar.gz"
tar -C "$BUILD" -czf "$TAR" "$APP_NAME.app"
printf 'Built macOS app tarball: %s\n' "$TAR"

if command -v ditto >/dev/null 2>&1; then
    ZIP="$DIST/${NAME}-${VERSION}-macos-${ARCH}.zip"
    ditto -c -k --sequesterRsrc --keepParent "$APP" "$ZIP"
    printf 'Built macOS app zip: %s\n' "$ZIP"
fi

if command -v hdiutil >/dev/null 2>&1; then
    DMG="$DIST/${NAME}-${VERSION}-macos-${ARCH}.dmg"
    rm -f "$DMG"
    hdiutil create -volname "$APP_NAME" -srcfolder "$APP" -ov -format UDZO "$DMG"
    printf 'Built macOS dmg: %s\n' "$DMG"
else
    printf 'Skipped dmg: hdiutil not found.\n'
fi
