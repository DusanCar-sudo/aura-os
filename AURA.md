# Aura Standing Rules

## What this repo is
Aura OS: your own Omarchy-style system on Arch + sway. You
build it and you operate it. Omarchy is inspiration, not a source
to copy; write our own scripts and cite ideas in commit messages.

## Where you run
Develop in this repo on the host. Test ONLY inside the VM:
ssh -i ~/.ssh/aura_vm_ed25519 -p 2222 dusan@localhost (sudo
needs no password there; sync with rsync to ~/aura-os). Reset
with snapper snapshot 1 "base-clean". Never run install steps, pacman,
systemctl or config writes against the host machine. The host is
Ubuntu, not Arch; if a command would touch it, stop and ask.

## System changes
Inside Aura OS every system change goes through a bin/aura-os-*
command. No raw sudo in chat. Each aura-os-* command must take a
snapper snapshot before it changes anything, be idempotent, and
print what it changed. If no command fits, write one first.

## Speed and RAM budget
Idle desktop after login, no apps: under 350 MB RAM. Key press
to window on screen: under 100 ms. No animations, no blur. No
Electron, Qt or Python daemons in the core. Prefer foot, fuzzel,
mako, greetd+tuigreet, zram. Event-driven, not polling. Every
change reports before/after idle RAM; over budget = bug.

## Daily driver
It must run everything Dusan runs today: Chrome, YouTube with
hardware video, Docker, dev toolchains, Unity, OBS, Steam. The
budget is the bare desktop; never cut compatibility to meet it.

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
