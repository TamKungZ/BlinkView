#!/usr/bin/env sh
set -eu

cargo build --release
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.local/share/thumbnailers" "$HOME/.local/share/icons/hicolor/1024x1024/apps"
install -m 0755 target/release/blinkview "$HOME/.local/bin/blinkview"
install -m 0644 assets/blinkview.png "$HOME/.local/share/icons/hicolor/1024x1024/apps/blinkview.png"

cat > "$HOME/.local/share/applications/blinkview.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=BlinkView
Comment=Fast folder-aware image/video viewer
Exec=$HOME/.local/bin/blinkview %f
TryExec=$HOME/.local/bin/blinkview
Icon=blinkview
Terminal=false
Categories=Graphics;Viewer;AudioVideo;
MimeType=image/png;image/jpeg;image/gif;image/bmp;image/webp;image/tiff;image/x-icon;image/x-portable-anymap;image/x-qoi;video/mp4;video/x-matroska;video/webm;video/quicktime;video/x-msvideo;video/mpeg;video/x-ms-wmv;video/x-flv;video/mp2t;
DESKTOP

# Freedesktop-compatible file managers may call this on demand. BlinkView
# writes PNG to %o and does not keep a thumbnail database of its own.
cat > "$HOME/.local/share/thumbnailers/blinkview.thumbnailer" <<THUMBNAILER
[Thumbnailer Entry]
TryExec=$HOME/.local/bin/blinkview
Exec=$HOME/.local/bin/blinkview --thumbnail %i %o %s
MimeType=image/png;image/jpeg;image/gif;image/bmp;image/webp;image/tiff;image/x-icon;image/x-portable-anymap;image/x-qoi;video/mp4;video/x-matroska;video/webm;video/quicktime;video/x-msvideo;video/mpeg;video/x-ms-wmv;video/x-flv;video/mp2t;
THUMBNAILER

"$HOME/.local/bin/blinkview" --startup enable
nohup "$HOME/.local/bin/blinkview" --background >/dev/null 2>&1 &
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$HOME/.local/share/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" || true
printf 'Installed BlinkView to %s\n' "$HOME/.local/bin/blinkview"
printf 'Background startup: enabled.\n'
printf 'Freedesktop thumbnailer: installed.\n'
printf 'This script does NOT force BlinkView as your default viewer.\n'
