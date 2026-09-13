#!/usr/bin/env bash
# run-iso-test.sh — boot the Aura adOS installer ISO against a throwaway
# disk, to test aura-install end to end before it touches real hardware.
#
#   vm/run-iso-test.sh <iso> <disk.img>          boot the ISO (installer)
#   vm/run-iso-test.sh --disk <disk.img>         boot the installed disk
#
# The disk should look like the target: an EFI partition, AURABOOT
# (FAT32, type ea00) and aura (btrfs). Drive it with
#   AURA_VM_SOCK=vm/isotest.sock vm/vm-keys.py key down / shot x.png
# Never run this while the main VM runs (host RAM, task 007).
set -euo pipefail
cd "$(dirname "$0")"

vars=isotest-OVMF_VARS.fd
[[ -f $vars ]] || cp /usr/share/OVMF/OVMF_VARS_4M.fd "$vars"
args=(
  -enable-kvm -machine q35,vmport=off -cpu host -smp 4 -m "${AURA_VM_RAM:-3G}"
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd
  -drive if=pflash,format=raw,file="$vars"
  -nic user,model=virtio-net-pci,hostfwd=tcp::2223-:22
  -device virtio-vga,xres=1920,yres=1200 -display "${AURA_VM_DISPLAY:-none}"
  -monitor unix:isotest.sock,server,nowait
  -device qemu-xhci -device usb-tablet
)
if [[ ${1:-} == --disk ]]; then
    args+=(-drive "file=${2:?disk},format=raw,if=virtio")
else
    args+=(-drive "file=${2:?disk},format=raw,if=virtio" -cdrom "${1:?iso}" -boot d)
fi
exec qemu-system-x86_64 "${args[@]}"
