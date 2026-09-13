#!/usr/bin/env bash
# 08-boot — the boot menu says "Aura OS". Safe to re-run.
#   - the kernel image (UKI) carries Aura OS's own os-release, so
#     systemd-boot titles it "Aura OS (<kernel>)". The system's
#     /etc/os-release stays Arch's: pacman and tools that check ID=arch
#     keep working.
#   - no Arch logo splash (THIRD-PARTY-NOTICES: no Arch marks).
#   - default = the current image, 3 s menu.
#   - a leftover "old system" UKI (from the 001b rollback test) becomes
#     one clearly named recovery entry instead of a second default.
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    exec sudo -E "$(readlink -f "$0")" "$@"
fi
REPO="${REPO_DIR:-$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)}"

osrel=/usr/share/aura-os/os-release
if ! cmp -s "$REPO/config/boot/os-release" "$osrel"; then
    install -Dm644 "$REPO/config/boot/os-release" "$osrel"
    echo "08-boot: installed $osrel"
fi

preset=/etc/mkinitcpio.d/linux.preset
want_uki=/boot/EFI/Linux/aura-os.efi
if ! grep -q "^default_uki=\"$want_uki\"" "$preset" || grep -q 'splash-arch' "$preset"; then
    command -v aura-os-snapshot >/dev/null && aura-os-snapshot "08-boot: before Aura OS boot branding" || true
    sed -i -E "s|^default_uki=.*|default_uki=\"$want_uki\"|;
               s|^default_options=.*|default_options=\"--osrelease $osrel\"|;
               s|^fallback_uki=.*|fallback_uki=\"/boot/EFI/Linux/aura-os-fallback.efi\"|;
               s|^#?fallback_options=.*|fallback_options=\"-S autodetect --osrelease $osrel\"|" "$preset"
    mkinitcpio -p linux >/dev/null
    echo "08-boot: rebuilt $want_uki"
    rm -f /boot/EFI/Linux/arch-linux.efi /boot/EFI/Linux/arch-linux-fallback.efi
fi

# The 001b rollback test left a pinned copy; keep it, but only as recovery.
if [[ -f /boot/EFI/Linux/zz-old-system.efi ]]; then
    install -d /boot/EFI/aura
    mv /boot/EFI/Linux/zz-old-system.efi /boot/EFI/aura/recovery.efi
    rm -f /boot/loader/entries/zz-old-system.conf
    printf 'title   Aura OS — recovery (older kernel image)\nefi     /EFI/aura/recovery.efi\nsort-key zz\n' \
        > /boot/loader/entries/aura-recovery.conf
    echo "08-boot: old system image → Aura OS recovery entry"
fi

conf=/boot/loader/loader.conf
want=$'default aura-os.efi\ntimeout 3\nconsole-mode keep\neditor no'
if [[ "$(cat "$conf" 2>/dev/null)" != "$want" ]]; then
    printf '%s\n' "$want" > "$conf"
    echo "08-boot: loader.conf → default aura-os.efi, 3 s"
fi
