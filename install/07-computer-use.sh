#!/usr/bin/env bash
# 07-computer-use — what Aura's :compon (computer use) needs on sway,
# and screen sharing in browsers too. Safe to re-run.
#   - screenshots: XDG ScreenCast portal → xdg-desktop-portal-wlr,
#     GStreamer + PipeWire, python-gobject, ImageMagick convert (PNG) (aura_screen.py)
#   - mouse/keyboard: Aura's own absolute uinput device (python-evdev);
#     /dev/uinput is opened to the logged-in seat user (uaccess), like
#     Steam Input does. Any process of that user can then inject input —
#     the trade-off computer use needs.
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    exec sudo -E "$(readlink -f "$0")" "$@"
fi

REPO="${REPO_DIR:-$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)}"
user="${AURA_USER:?AURA_USER is not set — run this through install.sh}"
home="$(getent passwd "$user" | cut -d: -f6)"

pacman -S --needed --noconfirm python-gobject python-evdev gst-plugins-base imagemagick \
    gst-plugin-pipewire xdg-desktop-portal xdg-desktop-portal-wlr xdg-desktop-portal-gtk >/dev/null

rule=/etc/udev/rules.d/70-aura-uinput.rules
if [[ ! -f $rule ]]; then
    printf 'KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"\n' > "$rule"
    printf 'uinput\n' > /etc/modules-load.d/aura-uinput.conf
    modprobe uinput
    udevadm control --reload
    udevadm trigger --name-match=uinput 2>/dev/null || udevadm trigger
    echo "07-computer-use: /dev/uinput opened to the seat user (uaccess)"
fi

install_user() {   # install_user <src> <dst>
    if ! cmp -s "$1" "$2"; then
        install -D -m 644 -o "$user" -g "$user" "$1" "$2"
        echo "07-computer-use: wrote $2"
    fi
}
install_user "$REPO/config/xdg-desktop-portal/sway-portals.conf" "$home/.config/xdg-desktop-portal/sway-portals.conf"
install_user "$REPO/config/xdg-desktop-portal-wlr/config" "$home/.config/xdg-desktop-portal-wlr/config"
