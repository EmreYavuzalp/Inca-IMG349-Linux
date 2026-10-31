#!/bin/sh
# Installs img349ctl from a release archive into ~/.local (or $PREFIX).
set -eu

cd "$(dirname "$0")"
PREFIX=${PREFIX:-"$HOME/.local"}

install -Dm 755 img349ctl "$PREFIX/bin/img349ctl"
install -Dm 755 img349ctl-gui "$PREFIX/bin/img349ctl-gui"
install -Dm 644 man/img349ctl.1 "$PREFIX/share/man/man1/img349ctl.1"
# Absolute path, so the menu entry works even if ~/.local/bin is not in PATH.
mkdir -p "$PREFIX/share/applications"
sed "s|^Exec=img349ctl-gui|Exec=$PREFIX/bin/img349ctl-gui|" contrib/img349ctl-gui.desktop \
    > "$PREFIX/share/applications/img349ctl-gui.desktop.tmp"
mv "$PREFIX/share/applications/img349ctl-gui.desktop.tmp" "$PREFIX/share/applications/img349ctl-gui.desktop"
echo "Kuruldu / installed: $PREFIX/bin/img349ctl, $PREFIX/bin/img349ctl-gui"

RULE=/etc/udev/rules.d/70-img349-mouse.rules
if [ ! -f "$RULE" ]; then
    echo
    echo "Fareye erişim izni için udev kuralı kuruluyor (sudo şifresi istenebilir)."
    echo "Installing the udev rule for mouse access (may ask for your sudo password)."
    sudo install -m 644 contrib/70-img349-mouse.rules "$RULE"
    sudo udevadm control --reload-rules
    sudo udevadm trigger --subsystem-match=hidraw
    echo "Fareyi çıkarıp tekrar takın. / Re-plug the mouse."
fi

case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *) echo "Not: $PREFIX/bin PATH içinde değil. / Note: $PREFIX/bin is not in PATH." ;;
esac
