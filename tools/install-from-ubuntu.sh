#!/usr/bin/env bash
# install-from-ubuntu — install Aura OS into its prepared partitions from
# the running Ubuntu on the same laptop. No USB stick: Ubuntu's kernel
# already drives the screen, so nothing floods the console, and the
# desktop (install/0*.sh) is built here too instead of on a first boot.
#
# Same result as the stick's aura-install, found by the same labels:
#   aura (btrfs, system)  AURABOOT (FAT32, /boot, XBOOTLDR)  + the ESP
#   Ubuntu has at /boot/efi (reused, never formatted).
# Only "aura" and "AURABOOT" are formatted. Ubuntu is not touched,
# except that its ESP gets systemd-boot, whose menu lists Aura + Ubuntu.
#
# Usage:  sudo bash tools/install-from-ubuntu.sh
# Log:    /var/log/aura-install-from-ubuntu.log
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    exec sudo bash "$(readlink -f "$0")" "$@"
fi

REPO="$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)"
LOG=/var/log/aura-install-from-ubuntu.log
W=/var/tmp/aura-bootstrap          # Arch bootstrap tree, kept for re-runs
B=$W/root.x86_64
M=$B/mnt                           # the new system, as the bootstrap sees it
MIRROR=https://geo.mirror.pkgbuild.com
NAME=dusan
HOST=aura

exec > >(tee -a "$LOG") 2>&1
echo "=== $(date -Is) install-from-ubuntu"

die()  { echo "install-from-ubuntu: $*" >&2; exit 1; }
step() { echo; echo "==> $*"; }

cleanup() {
    umount -R "$M" 2>/dev/null || true
    umount "$B" 2>/dev/null || true
}
trap cleanup EXIT

# ------------------------------------------------------------- checks
ROOT=$(blkid -L aura) || die "no partition labelled 'aura'"
BOOT=$(blkid -L AURABOOT) || die "no partition labelled 'AURABOOT'"
ESP=$(findmnt -no SOURCE /boot/efi) || die "Ubuntu's ESP is not mounted at /boot/efi"
DATA=$(blkid -L DATA 2>/dev/null || true)
BIG=$(blkid -L bigdata 2>/dev/null || true)
for p in "$ROOT" "$BOOT"; do
    findmnt -S "$p" >/dev/null && die "$p is mounted — unmount it first"
done
[[ $(findmnt -no SOURCE /) != "$ROOT" ]] || die "'aura' is the running root?!"
[[ -d /sys/firmware/efi ]] || die "not booted in UEFI mode"
[[ -f $REPO/install/03-sway.sh ]] || die "repo not found at $REPO"
ZONE=$(timedatectl show -p Timezone --value 2>/dev/null || echo UTC)

echo
echo "  formatted:  $ROOT ($(lsblk -no SIZE "$ROOT" | tr -d ' '), system)"
echo "              $BOOT ($(lsblk -no SIZE "$BOOT" | tr -d ' '), /boot)"
echo "  reused:     $ESP (Ubuntu's EFI partition, not formatted)"
echo "  user $NAME, host $HOST, time zone $ZONE, repo from $REPO"
echo
read -rp "  Type INSTALL to erase $ROOT and $BOOT and install Aura OS: " ok
[[ ${ok^^} == INSTALL ]] || die "cancelled — nothing changed"
while :; do
    read -rsp "  Password for $NAME (and root): " PASS; echo
    read -rsp "  Again: " p2; echo
    [[ -n $PASS && $PASS == "$p2" ]] && break
    echo "  empty or different — try again"
done
unset p2

# ------------------------------------------------------------- bootstrap
step "Arch bootstrap tree"
mkdir -p "$W"
if [[ ! -x $B/usr/bin/pacman ]]; then
    cd "$W"
    curl -fL --retry 3 -o bootstrap.tar.zst "$MIRROR/iso/latest/archlinux-bootstrap-x86_64.tar.zst"
    curl -fsSL -o sha256sums.txt "$MIRROR/iso/latest/sha256sums.txt"
    want=$(awk '$2 ~ /archlinux-bootstrap-x86_64\.tar\.zst$/ {print $1}' sha256sums.txt)
    [[ -n $want && $(sha256sum bootstrap.tar.zst | cut -d' ' -f1) == "$want" ]] \
        || die "bootstrap checksum mismatch"
    tar --zstd -xpf bootstrap.tar.zst --numeric-owner
    rm -f bootstrap.tar.zst
    cd - >/dev/null
fi
printf 'Server = %s/$repo/os/$arch\nServer = https://fastly.mirror.pkgbuild.com/$repo/os/$arch\n' \
    "$MIRROR" > "$B/etc/pacman.d/mirrorlist"
sed -i 's/^#\?ParallelDownloads.*/ParallelDownloads = 10/' "$B/etc/pacman.conf"
mountpoint -q "$B" || mount --bind "$B" "$B"   # pacman's space check wants a mount point

# ------------------------------------------------------------- disks
step "formatting and mounting $ROOT + $BOOT"
o=compress=zstd,noatime
mkfs.btrfs -f -L aura "$ROOT"
mkfs.fat -F32 -n AURABOOT "$BOOT"
mount "$ROOT" "$M"
for s in @ @home @log @pkg; do btrfs subvolume create "$M/$s"; done
umount "$M"
mount -o $o,subvol=@ "$ROOT" "$M"
mount --mkdir -o $o,subvol=@home "$ROOT" "$M/home"
mount --mkdir -o $o,subvol=@log "$ROOT" "$M/var/log"
mount --mkdir -o $o,subvol=@pkg "$ROOT" "$M/var/cache/pacman/pkg"
mount --mkdir "$BOOT" "$M/boot"
mount --mkdir "$ESP" "$M/efi"

# ------------------------------------------------------------- hardware
gpu="" ucode="" fw=()
lspci | grep -qiE '(vga|display).*(amd|ati)' && gpu="vulkan-radeon libva-mesa-driver"
lspci | grep -qiE 'vga.*intel' && gpu="vulkan-intel intel-media-driver"
grep -q AuthenticAMD /proc/cpuinfo && ucode=amd-ucode
grep -q GenuineIntel /proc/cpuinfo && ucode=intel-ucode
ids="$(lspci -n | awk '{print $3}'; lsusb 2>/dev/null | awk '{print $6}')"
grep -q '^1002:' <<<"$ids" && fw+=(linux-firmware-radeon)   # amdgpu: pinned below
grep -q AuthenticAMD /proc/cpuinfo && fw+=(linux-firmware-amd)
grep -q '^8086:' <<<"$ids" && fw+=(linux-firmware-intel)
grep -q '^10de:' <<<"$ids" && fw+=(linux-firmware-nvidia)
grep -qE '^(10ec|0bda):' <<<"$ids" && fw+=(linux-firmware-realtek)
grep -qE '^(14c3|0e8d):' <<<"$ids" && fw+=(linux-firmware-mediatek)
grep -qE '^(168c|17cb|0cf3):' <<<"$ids" && fw+=(linux-firmware-atheros)
grep -qE '^(14e4|0a5c):' <<<"$ids" && fw+=(linux-firmware-broadcom)
(( ${#fw[@]} )) || fw=(linux-firmware)
amdfw=""
grep -q '^1002:' <<<"$ids" && amdfw=1
echo "gpu: ${gpu:-none}  ucode: ${ucode:-none}  firmware: ${fw[*]}${amdfw:+ + amdgpu 20260810-2}"

# ------------------------------------------------------------- base system
# Runs inside the bootstrap tree; $M there is /mnt.
step "base system (pacstrap)"
cat > "$B/root/stage-base.sh" <<STAGE
set -euo pipefail
pacman-key --init
pacman-key --populate archlinux
pacstrap -K /mnt base linux ${fw[*]} $ucode btrfs-progs networkmanager sudo git vim \\
    openssh sof-firmware zram-generator man-db terminus-font mesa $gpu power-profiles-daemon rsync
# linux-firmware-amdgpu 20260910-1 breaks the Radeon 680M (DMCUB error
# flood). Install 20260810-2 and hold it until Arch ships a fixed one.
if [[ -n "$amdfw" ]]; then
    arch-chroot /mnt pacman -U --noconfirm \\
        https://archive.archlinux.org/packages/l/linux-firmware-amdgpu/linux-firmware-amdgpu-20260810-2-any.pkg.tar.zst
    sed -i 's/^#\\?IgnorePkg *=.*/IgnorePkg = linux-firmware-amdgpu/' /mnt/etc/pacman.conf
    grep -q '^IgnorePkg = linux-firmware-amdgpu' /mnt/etc/pacman.conf
fi
sed -i 's/^#\\?ParallelDownloads.*/ParallelDownloads = 10/' /mnt/etc/pacman.conf
genfstab -U /mnt >> /mnt/etc/fstab
STAGE
"$B/usr/bin/arch-chroot" "$B" bash /root/stage-base.sh

# ------------------------------------------------------------- settings
step "time, language, user, network"
{
    [[ -n $DATA ]] && echo "UUID=$(blkid -s UUID -o value "$DATA") /mnt/data    ext4 defaults,nofail 0 2"
    [[ -n $BIG ]]  && echo "UUID=$(blkid -s UUID -o value "$BIG") /mnt/bigdata ext4 defaults,nofail 0 2"
} >> "$M/etc/fstab"
mkdir -p "$M/mnt/data" "$M/mnt/bigdata"
printf 'KEYMAP=us\nFONT=ter-124b\n' > "$M/etc/vconsole.conf"
echo "$HOST" > "$M/etc/hostname"
sed -i 's/^#en_US.UTF-8 UTF-8/en_US.UTF-8 UTF-8/' "$M/etc/locale.gen"
echo 'LANG=en_US.UTF-8' > "$M/etc/locale.conf"
printf '[zram0]\nzram-size = ram / 2\ncompression-algorithm = zstd\n' > "$M/etc/systemd/zram-generator.conf"
echo '%wheel ALL=(ALL:ALL) ALL' > "$M/etc/sudoers.d/wheel"; chmod 440 "$M/etc/sudoers.d/wheel"

# the Wi-Fi Ubuntu is on right now works on the first boot too
wifi=$(nmcli -t -f NAME,TYPE con show --active 2>/dev/null | awk -F: '$2=="802-11-wireless"{print $1; exit}')
if [[ -n $wifi ]]; then
    src=$(grep -lx "id=$wifi" /etc/NetworkManager/system-connections/*.nmconnection 2>/dev/null | head -n1 || true)
    if [[ -n $src ]]; then
        install -Dm600 /dev/null "$M/etc/NetworkManager/system-connections/aura-wifi.nmconnection"
        grep -v '^permissions=' "$src" > "$M/etc/NetworkManager/system-connections/aura-wifi.nmconnection"
        echo "wifi: $wifi"
    fi
fi

T() { "$B/usr/bin/arch-chroot" "$B" arch-chroot /mnt "$@"; }
T sh -c "ln -sf /usr/share/zoneinfo/$ZONE /etc/localtime && hwclock --systohc && locale-gen"
T useradd -m -G wheel -s /bin/bash "$NAME"
printf '%s:%s\nroot:%s\n' "$NAME" "$PASS" "$PASS" | T chpasswd
unset PASS
T systemctl enable NetworkManager

# ------------------------------------------------------------- boot menu
step "boot menu (systemd-boot on the shared ESP; Ubuntu stays listed)"
echo "root=UUID=$(blkid -s UUID -o value "$ROOT") rootflags=subvol=@ rw quiet loglevel=3" > "$M/etc/kernel/cmdline"
sed -i -e 's/^default_image/#default_image/' -e 's/^fallback_image/#fallback_image/' \
    -e 's|^#default_uki=.*|default_uki="/boot/EFI/Linux/arch-linux.efi"|' \
    -e 's|^#fallback_uki=.*|fallback_uki="/boot/EFI/Linux/arch-linux-fallback.efi"|' \
    "$M/etc/mkinitcpio.d/linux.preset"
mkdir -p "$M/boot/EFI/Linux"
T mkinitcpio -P
rm -f "$M"/boot/initramfs-linux*.img
T bootctl --esp-path=/efi --boot-path=/boot install
mkdir -p "$M/efi/loader/entries"
if [[ -f $M/efi/EFI/ubuntu/shimx64.efi ]]; then
    printf 'title   Ubuntu\nefi     /EFI/ubuntu/shimx64.efi\n' > "$M/efi/loader/entries/ubuntu.conf"
elif [[ -f $M/efi/EFI/ubuntu/grubx64.efi ]]; then
    printf 'title   Ubuntu\nefi     /EFI/ubuntu/grubx64.efi\n' > "$M/efi/loader/entries/ubuntu.conf"
fi

# ------------------------------------------------------------- Aura OS
step "copying Aura OS + shared memories"
d=$M/home/$NAME/aura-os
rsync -a --exclude 'vm/*.qcow2' --exclude 'vm/*.iso' --exclude 'vm/*.fd' \
    --exclude 'term/build*' --exclude '.claude/' "$REPO/" "$d/"
mem=/mnt/bigdata/moved-from-system/home-$NAME
install -d "$M/home/$NAME/.claude/projects/-home-$NAME" "$M/home/$NAME/.aura"
ln -sfn "$mem/.claude/projects/-home-$NAME/memory" "$M/home/$NAME/.claude/projects/-home-$NAME/memory"
ln -sfn "$mem/.aura/memory" "$M/home/$NAME/.aura/memory"
T chown -R "$NAME:$NAME" "/home/$NAME"

# The desktop is built now, not on a first boot. 02-snapper can't see
# btrfs as / from a chroot, so it always waits for the first boot, as
# does any step that fails here.
step "the Aura desktop (install/0*.sh)"
later=(02)
for s in "$d"/install/0*.sh; do
    n=$(basename "$s" .sh)
    [[ $n == 02-* ]] && continue
    echo "--> $n"
    if T env AURA_USER="$NAME" AURA_FIRSTBOOT=1 REPO_DIR="/home/$NAME/aura-os" \
            bash "/home/$NAME/aura-os/install/$n.sh" </dev/null; then
        echo "+   $n"
    else
        echo "x   $n — retried on the first boot"
        later+=("${n%%-*}")
    fi
done
T chown -R "$NAME:$NAME" "/home/$NAME"

step "first boot: ${later[*]}"
install -Dm755 "$d/bin/aura-os-firstboot" "$M/usr/local/bin/aura-os-firstboot"
printf 'AURA_USER=%s\nAURA_FIRSTBOOT_STEPS=%s\n' "$NAME" "${later[*]}" > "$M/etc/aura-os-firstboot.env"
cat > "$M/etc/systemd/system/aura-os-firstboot.service" <<'UNIT'
[Unit]
Description=Aura adOS first boot — the steps the install left
After=network-online.target
Wants=network-online.target
Before=getty@tty1.service greetd.service
ConditionPathExists=/etc/aura-os-firstboot.env

[Service]
Type=oneshot
EnvironmentFile=/etc/aura-os-firstboot.env
ExecStart=/usr/local/bin/aura-os-firstboot
StandardInput=tty
StandardOutput=tty
TTYPath=/dev/tty1
TTYReset=yes
TimeoutSec=0

[Install]
WantedBy=multi-user.target
UNIT
T systemctl enable aura-os-firstboot NetworkManager-wait-online

sync
echo
echo "=== done. Reboot: the menu shows Aura OS (default) and Ubuntu."
echo "    First start runs step(s) ${later[*]}, restarts once, then log in as $NAME."
echo "    Log: $LOG"
