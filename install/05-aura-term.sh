#!/usr/bin/env bash
# 05-aura-term — Aura OS's terminal (task 005, docs/aura-term.md).
# Builds aura-term (PGO, like Arch's foot) only when it is missing or
# older than term/, installs the shell integration, and sources it from
# the user's ~/.bashrc. Safe to re-run.
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    exec sudo -E "$(readlink -f "$0")" "$@"
fi

REPO="${REPO_DIR:-$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)}"
user="${AURA_USER:?AURA_USER is not set — run this through install.sh}"
home="$(getent passwd "$user" | cut -d: -f6)"

# Build deps (see term/meson.build); --needed keeps this a no-op later.
pacman -S --needed --noconfirm meson ninja gcc pkgconf tllist scdoc \
    wayland-protocols fcft libxkbcommon pixman libutf8proc >/dev/null

bin=/usr/local/bin/aura-term
if [[ ! -x $bin ]] || [[ -n $(find "$REPO/term" -newer "$bin" -name '*.[ch]' -print -quit) ]]; then
    echo "05-aura-term: building aura-term (PGO) — a few minutes"
    sudo -u "$user" "$REPO/tools/aura-term-build.sh" pgo "$REPO/term" "$REPO/term/build-pgo"
    ninja -C "$REPO/term/build-pgo" install >/dev/null
    echo "05-aura-term: installed $bin"
else
    echo "05-aura-term: up to date: $bin"
fi

share=/usr/local/share/aura-term/shell
install -d "$share"
for f in aura-term.bash aura-term.zsh; do
    if ! cmp -s "$REPO/config/shell/$f" "$share/$f"; then
        install -m 644 "$REPO/config/shell/$f" "$share/$f"
        echo "05-aura-term: installed $share/$f"
    fi
done

install -m 644 "$REPO/config/shell/aura-os-recent.bash" "$share/aura-os-recent.bash"
line2=". $share/aura-os-recent.bash"
if ! grep -qxF "$line2" "$home/.bashrc" 2>/dev/null; then
    printf '# Aura OS: commands and folders for the sidebar recent list\n%s\n' "$line2" >> "$home/.bashrc"
    echo "05-aura-term: sourced aura-os-recent.bash from $home/.bashrc"
fi

line=". $share/aura-term.bash"
if ! grep -qxF "$line" "$home/.bashrc" 2>/dev/null; then
    printf '\n# aura-term: command blocks, cwd/branch, Ask Aura (task 005)\n%s\n' "$line" >> "$home/.bashrc"
    chown "$user:" "$home/.bashrc"
    echo "05-aura-term: sourced the shell integration from $home/.bashrc"
fi

# aura-term config for the user (a user file overrides the xdg default).
cfg="$home/.config/aura-term"
install -d -o "$user" -g "$user" "$cfg"
if ! cmp -s "$REPO/config/aura-term/aura-term.ini" "$cfg/aura-term.ini"; then
    install -m 644 -o "$user" -g "$user" "$REPO/config/aura-term/aura-term.ini" "$cfg/aura-term.ini"
    echo "05-aura-term: wrote $cfg/aura-term.ini (purplerain theme)"
fi
