# Aura OS

> Finally, your own OS on your own laptop — running what you need,
> nothing you don't. Aura built it for herself: she installs, themes,
> fixes and backs it up. You ask; Aura does it, snapshots first.

## Direction: a dev OS, not a polished one

Aura OS is a working desk for developers and their agents: text over
icons, square panes, every agent's last step in the sidebar, the
numbers that matter (RAM, the last 500 copies) one glance away. It runs
everything you run today — browsers, dev tools, media, games — on a
bare desktop under 350 MB. If it doesn't help you ship, it isn't on
screen.

## Two projects in one

1. **The OS** — a layer on Arch Linux: install script,
   sway dotfiles, `aura-os-*` commands, and Aura as the operator.
2. **Aura herself** — Aura builds most of this from instructions. Where she
   stumbles (wrong tool, wrong surface, runaway commands), the fix goes into
   aura-code, not just this repo. Log those in `NOTES-aura-behaviour.md`.

## Layout

    vm/            QEMU test VM (run-vm.sh, disk image, ISO — not in git)
    install.sh     entry point, run on a fresh Arch install   (Aura builds)
    install/       ordered install steps                      (Aura builds)
    config/        sway, Waybar, fuzzel, mako, foot          (Aura builds)
    bin/aura-os-*  the only commands Aura may use for system changes
    themes/        swappable themes
    AURA.md        standing rules Aura loads for this repo

## Test loop

    cd vm && ./run-vm.sh install    # first boot: archinstall, then clone repo
    ./run-vm.sh                     # later boots
    ssh -p 2222 dusan@localhost     # drive the guest from the host
