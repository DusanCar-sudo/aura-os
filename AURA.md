# Aura Standing Rules

## What this repo is
Aura OS: your own Omarchy-style system on Arch + Hyprland. You
build it and you operate it. Omarchy is inspiration, not a source
to copy; write our own scripts and cite ideas in commit messages.

## Where you run
Develop in this repo on the host. Test ONLY inside the VM
(vm/run-vm.sh, ssh -p 2222). Never run install steps, pacman,
systemctl or config writes against the host machine. The host is
Ubuntu, not Arch; if a command would touch it, stop and ask.

## System changes
Inside Aura OS every system change goes through a bin/aura-os-*
command. No raw sudo in chat. Each aura-os-* command must take a
snapper snapshot before it changes anything, be idempotent, and
print what it changed. If no command fits, write one first.

## Install scripts
install.sh runs numbered steps in install/. Each step must be
safe to re-run. Use set -euo pipefail. No curl | bash from
unknown hosts. Pin nothing you cannot update.

## Secrets
Never read, print or commit ~/.aura/*.env, tokens or keys. If a
task needs one, reference the variable name only.

## Done means tested
A change is done when it works in a fresh VM boot, not when the
script exits 0. Say what you verified and what you did not.
