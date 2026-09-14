# Aura Standing Rules

## What this repo is
Aura OS: finally your own OS on your own laptop, Arch + sway,
running what the user needs. You build it and you operate it.
Follow instincts and trends; write our own scripts, never copy
another distro's code.

## Where you run
This machine (hostname `aura`, Lenovo IdeaPad Slim 5 15ARP10)
is the real Aura OS install, bare metal — no VM anymore (the
old dev-VM at port 2222 is gone, don't reference it). It
dual-boots an older Ubuntu, mounted rw at /mnt/oldlinux when
Aura OS is booted — reference/transfer files only, never boot
or modify it from scripts. No disposable test target left:
every change here is live. Reset with snapper snapshot 1
"base-clean" if it breaks.

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
A change is done when it works after a real reboot, not when
the script exits 0. Say what you verified and what you did not.
