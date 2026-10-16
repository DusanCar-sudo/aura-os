#!/usr/bin/env bash
# 10-grub-theme — the Aura boot menu.
#
#   - themes GRUB with the Aura art (assets/grub-theme: artwork from
#     tools/make-splash.py, fonts made here with grub-mkfont).
#   - puts "Aura OS" FIRST in the menu, chainloading Aura's own UKI, so the
#     machine boots Aura unless someone picks otherwise.
#
# Safe to re-run. Only touches the GRUB it finds at $GRUB_DIR.
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    exec sudo -E "$(readlink -f "$0")" "$@"
fi
REPO="${REPO_DIR:-$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)}"

SRC="$REPO/assets/grub-theme"
GRUB_DIR=/boot/grub
THEME_DIR="$GRUB_DIR/themes/aura"
DEFAULTS=/etc/default/grub

# Aura's UKI, wherever its own ESP is mounted
uki=
for p in /boot/EFI/Linux/aura-os.efi /efi/EFI/Linux/aura-os.efi; do
    [[ -f $p ]] && { uki=$p; break; }
done
if [[ -z $uki ]]; then
    echo "10-grub-theme: no Aura UKI (aura-os.efi) found; run install/08-boot.sh first" >&2
    exit 1
fi
uki_uuid=$(findmnt -no UUID -T "$uki")

# 1. the art, the theme, the row pixmaps
install -d "$THEME_DIR/icons"
install -m644 "$SRC/background.png" "$THEME_DIR/background.png"
install -m644 "$SRC/theme.txt" "$THEME_DIR/theme.txt"
install -m644 "$SRC"/icons/*.png "$THEME_DIR/icons/"
install -m644 "$SRC"/select_*.png "$THEME_DIR/"

# 2. the fonts theme.txt asks for by name (-n), so GRUB can resolve them
install -d "$THEME_DIR/fonts"
if command -v grub-mkfont >/dev/null; then
    grub-mkfont -s 28 -n "Aura 28" -o "$THEME_DIR/fonts/aura-28.pf2" "$SRC/fonts/Geist-SemiBold.ttf"
    grub-mkfont -s 17 -n "Aura 17" -o "$THEME_DIR/fonts/aura-17.pf2" "$SRC/fonts/Geist-Regular.ttf"
else
    echo "10-grub-theme: grub-mkfont missing, keeping existing .pf2" >&2
fi

# 3. Aura OS first in the menu. 09_ runs before 10_linux, so it lands on top,
#    which keeps GRUB_DEFAULT=0 meaning Aura.
cat > /etc/grub.d/09_aura <<EOF
#!/bin/sh
exec tail -n +3 \$0
menuentry "Aura OS" --class aura --class os {
	insmod part_gpt
	insmod fat
	insmod chain
	search --no-floppy --fs-uuid --set=root $uki_uuid
	chainloader /EFI/Linux/aura-os.efi
}
EOF
chmod 755 /etc/grub.d/09_aura

# 4. point GRUB at the theme; a fixed mode keeps the 1920x1080 art undistorted.
#    GRUB_DISTRIBUTOR renames the host's own kernels, so only claim it on an
#    actual Aura OS host — on a dual-boot neighbour the explicit "Aura OS"
#    entry above is the one that must say Aura.
snapshot() { command -v aura-os-snapshot >/dev/null && aura-os-snapshot "$1" || true; }
dist=; grep -q '^ID=auraos' /etc/os-release 2>/dev/null && dist='s|^GRUB_DISTRIBUTOR=.*|GRUB_DISTRIBUTOR="Aura OS"|;'
if ! grep -q "^GRUB_THEME=\"$THEME_DIR/theme.txt\"" "$DEFAULTS"; then
    snapshot "10-grub-theme: before the Aura boot theme"
    sed -i -E "s|^#?GRUB_THEME=.*|GRUB_THEME=\"$THEME_DIR/theme.txt\"|;
               s|^#?GRUB_TIMEOUT_STYLE=.*|GRUB_TIMEOUT_STYLE=menu|;
               s|^#?GRUB_GFXMODE=.*|GRUB_GFXMODE=1920x1080|;
               $dist" "$DEFAULTS"
    grep -q "^GRUB_THEME=" "$DEFAULTS" || echo "GRUB_THEME=\"$THEME_DIR/theme.txt\"" >> "$DEFAULTS"
fi

# 5. build the menu
grub-mkconfig -o "$GRUB_DIR/grub.cfg" >/dev/null

# self-check: the menu it just built actually loads the theme and Aura
grep -q "$THEME_DIR/theme.txt" "$GRUB_DIR/grub.cfg" || { echo "10-grub-theme: theme not in grub.cfg" >&2; exit 1; }
grep -q 'menuentry "Aura OS"' "$GRUB_DIR/grub.cfg" || { echo "10-grub-theme: Aura OS entry missing" >&2; exit 1; }
[[ -f $THEME_DIR/fonts/aura-28.pf2 ]] || { echo "10-grub-theme: menu font missing" >&2; exit 1; }
echo "10-grub-theme: Aura OS boot menu installed ($uki_uuid)"
