#!/bin/sh
# Builds img349ctl_<version>_amd64.deb from already compiled release binaries.
# usage: contrib/build-deb.sh <version> <dir with img349ctl and img349ctl-gui> [output dir]
set -eu

VERSION=$1
BIN=$2
OUT=${3:-.}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
umask 022
PKG=$(mktemp -d)
trap 'rm -rf "$PKG"' EXIT
chmod 755 "$PKG"  # mktemp makes it 0700; it becomes / on install

install -Dm 755 "$BIN/img349ctl" "$PKG/usr/bin/img349ctl"
install -Dm 755 "$BIN/img349ctl-gui" "$PKG/usr/bin/img349ctl-gui"
install -Dm 644 "$ROOT/contrib/70-img349-mouse.rules" "$PKG/usr/lib/udev/rules.d/70-img349-mouse.rules"
install -Dm 644 "$ROOT/contrib/img349ctl-gui.desktop" "$PKG/usr/share/applications/img349ctl-gui.desktop"
install -d "$PKG/usr/share/man/man1"
gzip -9n < "$ROOT/man/img349ctl.1" > "$PKG/usr/share/man/man1/img349ctl.1.gz"
install -Dm 644 "$ROOT/LICENSE" "$PKG/usr/share/doc/img349ctl/copyright"
install -Dm 644 "$ROOT/README.md" "$PKG/usr/share/doc/img349ctl/README.md"
install -Dm 644 "$ROOT/README.en.md" "$PKG/usr/share/doc/img349ctl/README.en.md"
if [ -f "$ROOT/THIRD-PARTY-LICENSES" ]; then
    gzip -9n < "$ROOT/THIRD-PARTY-LICENSES" > "$PKG/usr/share/doc/img349ctl/THIRD-PARTY-LICENSES.gz"
fi

install -d "$PKG/DEBIAN"
SIZE=$(du -sk --exclude=DEBIAN "$PKG" | cut -f1)
cat > "$PKG/DEBIAN/control" <<EOF
Package: img349ctl
Version: $VERSION
Architecture: amd64
Maintainer: Emre Yavuzalp <emreyavuzalp2@gmail.com>
Installed-Size: $SIZE
Depends: libc6 (>= 2.29), libgcc-s1
Recommends: libgl1, libegl1, libxkbcommon0, libwayland-client0 | libx11-6
Section: utils
Priority: optional
Homepage: https://github.com/EmreYavuzalp/Inca-IMG349-Linux
Description: Inca IMG-349 gaming mouse configuration tool
 Configure the Inca IMG-349 gaming mouse on Linux without the vendor's
 Windows software: DPI levels and colours, LED mode, button bindings
 and polling rate.
 .
 Includes the img349ctl command line tool, the img349ctl-gui graphical
 app and a udev rule that lets the logged-in user access the mouse.
EOF

cat > "$PKG/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = configure ] && command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules || true
    udevadm trigger --subsystem-match=hidraw --action=add || true
fi
EOF
cat > "$PKG/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = remove ] && command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules || true
fi
EOF
chmod 755 "$PKG/DEBIAN/postinst" "$PKG/DEBIAN/postrm"

mkdir -p "$OUT"
dpkg-deb --root-owner-group -Zxz --build "$PKG" "$OUT/img349ctl_${VERSION}_amd64.deb"
