#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

NAME=blinkview
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
HOST_ARCH=$(uname -m)
case "$HOST_ARCH" in
    x86_64|amd64) DEB_ARCH=amd64; RPM_ARCH=x86_64 ;;
    aarch64|arm64) DEB_ARCH=arm64; RPM_ARCH=aarch64 ;;
    *) DEB_ARCH="$HOST_ARCH"; RPM_ARCH="$HOST_ARCH" ;;
esac

DIST="$ROOT/dist"
BUILD=$(mktemp -d "${TMPDIR:-/tmp}/blinkview-package.XXXXXX")
trap 'rm -rf "$BUILD"' EXIT INT TERM
STAGE="$BUILD/${NAME}-${VERSION}-linux-${HOST_ARCH}"

cargo build --release
rm -rf "$STAGE"
mkdir -p \
    "$STAGE/usr/bin" \
    "$STAGE/usr/share/applications" \
    "$STAGE/usr/share/thumbnailers" \
    "$STAGE/usr/share/icons/hicolor/1024x1024/apps" \
    "$STAGE/usr/share/doc/$NAME"

install -m 0755 target/release/blinkview "$STAGE/usr/bin/blinkview"
install -m 0644 assets/blinkview.png "$STAGE/usr/share/icons/hicolor/1024x1024/apps/blinkview.png"
install -m 0644 README.md CHANGELOG.md LICENSE "$STAGE/usr/share/doc/$NAME/"

cat > "$STAGE/usr/share/applications/blinkview.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=BlinkView
Comment=Fast folder-aware image/video viewer
Exec=blinkview %f
TryExec=blinkview
Icon=blinkview
Terminal=false
Categories=Graphics;Viewer;AudioVideo;
MimeType=image/png;image/jpeg;image/gif;image/bmp;image/webp;image/tiff;image/x-icon;image/x-portable-anymap;image/x-qoi;video/mp4;video/x-matroska;video/webm;video/quicktime;video/x-msvideo;video/mpeg;video/x-ms-wmv;video/x-flv;video/mp2t;
DESKTOP

cat > "$STAGE/usr/share/thumbnailers/blinkview.thumbnailer" <<'THUMBNAILER'
[Thumbnailer Entry]
TryExec=blinkview
Exec=blinkview --thumbnail %i %o %s
MimeType=image/png;image/jpeg;image/gif;image/bmp;image/webp;image/tiff;image/x-icon;image/x-portable-anymap;image/x-qoi;video/mp4;video/x-matroska;video/webm;video/quicktime;video/x-msvideo;video/mpeg;video/x-ms-wmv;video/x-flv;video/mp2t;
THUMBNAILER

mkdir -p "$DIST"
TAR="$DIST/${NAME}-${VERSION}-linux-gnu-${HOST_ARCH}.tar.gz"
tar -C "$STAGE" --owner=0 --group=0 -czf "$TAR" .
printf 'Built tarball: %s\n' "$TAR"

DEBROOT="$BUILD/debroot"
rm -rf "$DEBROOT"
mkdir -p "$DEBROOT/DEBIAN"
cp -a "$STAGE/." "$DEBROOT/"
chmod 0755 "$DEBROOT" "$DEBROOT/DEBIAN"
INSTALLED_SIZE=$(du -ks "$DEBROOT/usr" | awk '{print $1}')
cat > "$DEBROOT/DEBIAN/control" <<DEB
Package: $NAME
Version: $VERSION
Section: graphics
Priority: optional
Architecture: $DEB_ARCH
Installed-Size: $INSTALLED_SIZE
Maintainer: TamKungZ_ <dev@tamkungz.me>
Description: Tiny fast image/video folder viewer
 BlinkView opens one media file and quickly navigates neighboring
 files in the same folder with bounded preloading.
DEB
DEB="$DIST/${NAME}_${VERSION}_${DEB_ARCH}.deb"
dpkg-deb --build --root-owner-group "$DEBROOT" "$DEB"
printf 'Built deb: %s\n' "$DEB"

if command -v rpmbuild >/dev/null 2>&1; then
    RPMTOP="$BUILD/rpmbuild"
    rm -rf "$RPMTOP"
    mkdir -p "$RPMTOP/BUILD" "$RPMTOP/BUILDROOT" "$RPMTOP/RPMS" "$RPMTOP/SOURCES" "$RPMTOP/SPECS" "$RPMTOP/SRPMS" "$RPMTOP/tmp"
    RPM_SOURCE="$RPMTOP/SOURCES/${NAME}-${VERSION}.tar.gz"
    tar -C "$STAGE" -czf "$RPM_SOURCE" .
    cat > "$RPMTOP/SPECS/$NAME.spec" <<RPM
Name: $NAME
Version: $VERSION
Release: 1%{?dist}
Summary: Tiny fast image/video folder viewer
License: MIT
Source0: %{name}-%{version}.tar.gz

%description
BlinkView opens one media file and quickly navigates neighboring files in the same folder.

%prep
mkdir -p %{_builddir}/%{name}-%{version}
tar -C %{_builddir}/%{name}-%{version} -xzf %{SOURCE0}

%build

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}
cp -a %{_builddir}/%{name}-%{version}/. %{buildroot}/

%files
%license /usr/share/doc/%{name}/LICENSE
%doc /usr/share/doc/%{name}/README.md
%doc /usr/share/doc/%{name}/CHANGELOG.md
/usr/bin/blinkview
/usr/share/applications/blinkview.desktop
/usr/share/thumbnailers/blinkview.thumbnailer
/usr/share/icons/hicolor/1024x1024/apps/blinkview.png
RPM
    rpmbuild \
        --define "_topdir $RPMTOP" \
        --define "_tmppath $RPMTOP/tmp" \
        --target "$RPM_ARCH" \
        -bb "$RPMTOP/SPECS/$NAME.spec"
    find "$RPMTOP/RPMS" -name '*.rpm' -exec cp {} "$DIST/" \;
    printf 'Built rpm(s):\n'
    find "$DIST" -maxdepth 1 -name "${NAME}-${VERSION}-*.rpm" -print
else
    printf 'Skipped rpm: rpmbuild not found.\n'
fi
