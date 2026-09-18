#!/usr/bin/env bash
# 04-aura — install the aura-os-* commands into /usr/local/bin.
# Safe to re-run: only files that differ are copied.
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    exec sudo -E "$(readlink -f "$0")" "$@"
fi

REPO="${REPO_DIR:-$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)}"

for src in "$REPO"/bin/aura-os-*; do
    dst="/usr/local/bin/$(basename "$src")"
    if [[ ! -e $dst ]] || ! cmp -s "$src" "$dst"; then
        install -m 755 "$src" "$dst"
        echo "04-aura: installed $dst"
    else
        echo "04-aura: up to date: $dst"
    fi
done

# Color combos (aura-os-theme reads /usr/share/aura-os/themes).
install -d /usr/share/aura-os/themes /usr/share/aura-os/wallpapers
for t in /usr/share/aura-os/themes/*/; do       # drop themes removed/renamed in the repo
    [[ -d $t && ! -d "$REPO/themes/$(basename "$t")" ]] && rm -rf -- "$t" && echo "04-aura: removed theme $(basename "$t")"
done
for t in "$REPO"/themes/*/; do
    t="$(basename "$t")"
    if ! cmp -s "$REPO/themes/$t/palette" "/usr/share/aura-os/themes/$t/palette"; then
        install -Dm644 "$REPO/themes/$t/palette" "/usr/share/aura-os/themes/$t/palette"
        echo "04-aura: installed theme $t"
    fi
done

# Icons (aura-os-theme recolors copies into ~/.local/share/aura-os/icons).
install -d /usr/share/aura-os/icons
for i in "$REPO"/assets/icons/*.svg; do
    cmp -s "$i" "/usr/share/aura-os/icons/$(basename "$i")" \
        || { install -Dm644 "$i" "/usr/share/aura-os/icons/$(basename "$i")"; echo "04-aura: installed icon $(basename "$i")"; }
done
