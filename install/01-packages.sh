#!/usr/bin/env bash
# 01-packages — the packages a minimal Aura OS desktop needs.
# sway + Waybar + launcher + terminal (+ notifications, a font, the
# wlroots portal, greetd). Packages the Hyprland-era setup replaced
# are removed through bin/aura-os-remove, which snapshots first and
# prints what went away. Safe to re-run.
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    exec sudo -E "$(readlink -f "$0")" "$@"
fi

REPO="${REPO_DIR:-$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)}"

packages=(
    sway                     # the compositor
    waybar                   # status bar
    fuzzel                   # launcher (Super+D)
    foot                     # terminal (Super+Return)
    swaybg                   # solid theme background (sway `output bg`), ~3 MB
    mako                     # notifications daemon (also serves Electron/Chrome)
    inotify-tools            # inotifywait: aura-os-agents follows agent logs
    figlet                   # big month title in aura-os-calendar
    sqlite3                  # read Firefox history for the sidebar's recent list
    cliphist wl-clipboard    # clipboard history, last 500 copies (top bar "clip")
    wiremix                  # volume TUI (top bar "vol" click)
    rtkit                    # realtime priority for pipewire: no crackling
    bluez bluez-utils        # bluetooth + bluetoothctl (top bar "bt")
    libnotify                # notify-send: mako's CLI; aura-term needs it for
                             # "agent needs input" alerts (task 005)
    jq                       # aura-os-tab / aura-os-status JSON (task 003)
    ttf-jetbrains-mono-nerd  # herdr theme: monospace everywhere (task 003)
    xdg-desktop-portal-wlr   # portal backend for wlroots compositors
    greetd                   # display manager (replaces sddm)
    greetd-tuigreet          # text-mode greeter — fits the RAM budget
    xorg-xwayland            # X11 apps via lazy Xwayland (task 001b)
    polkit-gnome             # polkit agent, started on demand only
)

# Packages this environment replaces. sddm is not here: 03-sway.sh
# swaps display managers first and retires it afterwards, because
# sddm owns the running session until greetd is up.
replaced=(
    hyprland
    hyprland-guiutils
    xdg-desktop-portal-hyprland
    kitty
    kitty-shell-integration
    kitty-terminfo
    uwsm                     # Hyprland session shim (and it is python)
    polkit-kde-agent
)

echo "01-packages: full system upgrade first (fresh install, current mirrors)"
pacman -Syu --noconfirm

before="$(pacman -Qq | LC_ALL=C sort)"
pacman -S --needed --noconfirm "${packages[@]}"
after="$(pacman -Qq | LC_ALL=C sort)"

added="$(LC_ALL=C comm -13 <(echo "$before") <(echo "$after") || true)"
if [[ -n $added ]]; then
    echo "01-packages: newly installed:"
    sed 's/^/  /' <<<"$added"
else
    echo "01-packages: nothing new — already complete"
fi

if [[ -x "$REPO/bin/aura-os-remove" ]]; then
    bash "$REPO/bin/aura-os-remove" "${replaced[@]}"
else
    echo "01-packages: WARNING: bin/aura-os-remove missing — old packages left alone" >&2
fi

# bluetooth for the top bar "bt" item (starts at next boot; harmless
# without a controller — waybar then hides the item).
if ! systemctl is-enabled --quiet bluetooth.service 2>/dev/null; then
    systemctl enable bluetooth.service
    echo "01-packages: enabled bluetooth.service"
fi
