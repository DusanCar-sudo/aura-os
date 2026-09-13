#!/usr/bin/env bash
# iso-build.sh — build the Aura adOS installer ISO (task 007). Runs on Arch
# (the VM): needs archiso. Starts from Arch's releng profile and changes
# only what Aura needs — every change is listed below.
#
#   tools/iso-build.sh [out-dir]      default ~/iso-out
#
#   1. names: ISO "auraos", label AURAOS_YYYYMM, boot entries
#      "Aura adOS installer" (wording "based on Arch Linux" only)
#   2. aura-install starts by itself on tty1 (root's .zlogin)
#   3. the Aura OS source rides along in /usr/share/aura-os-src, used when
#      no DATA partition with the repo is present
set -euo pipefail

REPO="$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)"
out="${1:-$HOME/iso-out}"
prof="$HOME/aura-iso-profile"
work="$HOME/aura-iso-work"          # on disk: /tmp is RAM in the VM

command -v mkarchiso >/dev/null || { echo "iso-build: install archiso first" >&2; exit 1; }
sudo rm -rf "$prof" "$work"
cp -r /usr/share/archiso/configs/releng "$prof"

# 1. names
label="AURAOS_$(date +%Y%m)"
sed -i -E "s|^iso_name=.*|iso_name=\"auraos\"|;
           s|^iso_label=.*|iso_label=\"$label\"|;
           s|^iso_publisher=.*|iso_publisher=\"Aura adOS\"|;
           s|^iso_application=.*|iso_application=\"Aura adOS installer (based on Arch Linux)\"|" "$prof/profiledef.sh"
grep -rl 'Arch Linux install medium' "$prof"/{efiboot,grub,syslinux} 2>/dev/null \
    | xargs -r sed -i 's/Arch Linux install medium/Aura adOS installer/g'

# 2. the installer + auto-start (+ lspci, which it uses to pick GPU drivers)
grep -qx pciutils "$prof/packages.x86_64" || echo pciutils >> "$prof/packages.x86_64"
cp -a "$REPO/iso/airootfs/." "$prof/airootfs/"
sed -i 's|^file_permissions=(|file_permissions=(\n  ["/usr/local/bin/aura-install"]="0:0:755"|' "$prof/profiledef.sh"
cat >> "$prof/airootfs/root/.zlogin" <<'EOF'

# Aura adOS: the installer on the first console
if [[ $(tty) == /dev/tty1 && -z ${AURA_INSTALL_DONE:-} ]]; then
    export AURA_INSTALL_DONE=1
    /usr/local/bin/aura-install
fi
EOF

# 3. the source, without the VM disk and build dirs
mkdir -p "$prof/airootfs/usr/share/aura-os-src"
rsync -a --exclude 'vm/' --exclude 'term/build*' --exclude '.git/' --exclude 'iso/' \
    "$REPO/" "$prof/airootfs/usr/share/aura-os-src/"

mkdir -p "$out"
sudo mkarchiso -v -w "$work" -o "$out" "$prof"
sudo rm -rf "$work"
cd "$out" && sha256sum auraos-*.iso > SHA256SUMS && ls -lh auraos-*.iso && cat SHA256SUMS
