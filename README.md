# Aura OS

> Aura looked at Omarchy — DHH's opinionated Arch + Hyprland setup — and
> decided she wanted a home of her own. Not a distro for people who tweak
> dotfiles, but a system she runs: installs, themes, fixes, backs up.
> You ask; Aura does it, snapshots first, and every change is a commit.

## Two projects in one

1. **The OS** — an Omarchy-style layer on Arch Linux: install script,
   Hyprland dotfiles, `aura-os-*` commands, and Aura as the operator.
2. **Aura herself** — Aura builds most of this from instructions. Where she
   stumbles (wrong tool, wrong surface, runaway commands), the fix goes into
   aura-code, not just this repo. Log those in `NOTES-aura-behaviour.md`.

## Layout

    vm/            QEMU test VM (run-vm.sh, disk image, ISO — not in git)
    install.sh     entry point, run on a fresh Arch install   (Aura builds)
    install/       ordered install steps                      (Aura builds)
    config/        Hyprland, Waybar, Walker, Mako, terminal   (Aura builds)
    bin/aura-os-*  the only commands Aura may use for system changes
    themes/        swappable themes
    AURA.md        standing rules Aura loads for this repo

## Test loop

    cd vm && ./run-vm.sh install    # first boot: archinstall, then clone repo
    ./run-vm.sh                     # later boots
    ssh -p 2222 dusan@localhost     # drive the guest from the host
