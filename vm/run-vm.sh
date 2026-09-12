#!/usr/bin/env bash
# Aura OS test VM — plain QEMU/KVM, no libvirt or sudo needed.
#   ./run-vm.sh install   boot the Arch ISO (first install)
#   ./run-vm.sh           boot the installed disk
# SSH into the guest: ssh -p 2222 <user>@localhost
set -euo pipefail
cd "$(dirname "$0")"

RAM="${AURA_VM_RAM:-4G}"   # host has 12G; don't go higher while LM Studio is loaded
CPUS="${AURA_VM_CPUS:-4}"

[ -f OVMF_VARS.fd ] || cp /usr/share/OVMF/OVMF_VARS_4M.fd OVMF_VARS.fd

args=(
  -enable-kvm -machine q35 -cpu host -smp "$CPUS" -m "$RAM"
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd
  -drive if=pflash,format=raw,file=OVMF_VARS.fd
  -drive file=aura-os.qcow2,if=virtio,discard=unmap
  -nic user,model=virtio-net-pci,hostfwd=tcp::2222-:22
  -device virtio-vga -display gtk,zoom-to-fit=on,grab-on-hover=on   # no GL: screendump works
  -monitor unix:monitor.sock,server,nowait   # host control: ./vm-keys.py
  -audiodev pipewire,id=snd0 -device intel-hda -device hda-duplex,audiodev=snd0
  -device qemu-xhci -device usb-tablet
)
[ "${1:-}" = install ] && args+=(-cdrom archlinux-x86_64.iso -boot d)

exec qemu-system-x86_64 "${args[@]}"
