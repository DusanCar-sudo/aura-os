# 008 — make Aura OS easy to ship: small core + plugins

Agreed with Dusan 2026-09-17. The OS is gear, like alpine shoes: it
helps the climb and never makes you look at your feet.

## 1. One source of truth  (started 2026-09-17)
- [x] Bring live-only edits into the repo (commit e62fc0bd).
- [x] tools/aura-os-drift — lists repo vs running-system differences.
- [ ] Redeploy repo-newer scripts to /usr/local/bin (needs sudo).
- [ ] Merge aura-os-shell into this repo (two repos today).
- [ ] One theme picker instead of two (aura-os-theme vs aura-theme).
- [ ] No hand edits in ~/.config or /usr/local/bin — deploy only.

## 2. Core (always there, never unplugged)
kernel, SwayFX, bar, Files, Settings, snapshots, Aura agent, Wi-Fi and
network, Bluetooth (people's only mouse/keyboard may be Bluetooth),
audio, language + keyboard, battery/power, one default theme.
Rule: if losing it could stop someone using or fixing the computer,
it's core.

## 3. Plugins (plug in when used, out when not)
One plugin = packages + settings page + launcher entries + services +
bar widget + keybindings, installed and removed together, snapshot
first. States: on / paused (no RAM) / off. Examples: themes, gaming,
printing, video, dev tools, Docker, local LLM, Office.
Aura suggests plugging in/out. Plugins become the Imprint tools list.

## 4. Packages + own repo
aura-core, aura-plugin-*; own pacman repo with SwayFX/scenefx built
by CI. Updates = pacman -Syu with snapshot + boot check + auto rollback.

## 5. Installer
Same aura-os-install behind: curl one-liner on Arch, ISO, from Ubuntu.
Asks only name, password, Wi-Fi, and optionally agent Base URL + Model
ID + API key (key belongs to that user only — owner's responsibility).
First login: Aura offers to set up the system; every step skippable.

## 6. CI
Build ISO → install in QEMU → boot → screenshot → aura-doctor. Fails,
doesn't ship.
