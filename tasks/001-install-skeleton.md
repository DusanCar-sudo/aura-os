# Task 001 — install skeleton

You are building Aura OS for yourself. Read README.md and AURA.md first.

## Goal
On a fresh Arch install inside the VM, `./install.sh` turns it into a
minimal Aura OS desktop: Hyprland + Waybar + a launcher + a terminal,
and a Super+A keybinding that opens an Aura prompt.

## Deliver
- `install.sh` running numbered, re-runnable steps from `install/`
  (01-packages, 02-snapper, 03-hyprland, 04-aura).
- `config/hypr/hyprland.conf` with Super+A bound to the Aura prompt.
- `bin/aura-os-snapshot` — takes a snapper snapshot with a message;
  every other aura-os-* command will call it first.
- `bin/aura-os-install <pkg>` — snapshot, then pacman -S, then print
  what changed.

## Rules
- Write code on the host, run it only in the VM over ssh -p 2222.
- Look at how Omarchy structures install/ for ideas; write our own.
- Report at the end: what you tested in the VM, what you did not.
