# Prompt for the Claude Code session building aura-term

You are building **aura-term**, the native terminal of Aura OS. Work
autonomously; stop and ask Dusan only for decisions listed under
"Ask first".

## Context (read these first, in this order)
1. `/run/media/dusan/DATA1/aura-os-term/README.md` — what Aura OS is.
2. `.../aura-os-term/AURA.md` — house rules. They apply to you too:
   test only in the VM, never change the host system, speed/RAM
   budget, secrets.
3. `.../aura-os-term/tasks/005-aura-terminal.md` — **the spec. Build
   exactly this.** Other tasks/ files are background only.
4. `/mnt/bigdata/aura/aura-code/docs/PROTOCOL.md` — Aura's sidecar
   protocol (NDJSON over stdio via `aura sidecar`), for "Ask Aura".

## Where you work
- **Worktree:** `/run/media/dusan/DATA1/aura-os-term`, branch
  `aura-term`. Commit there freely (small commits, clear messages).
- **Never touch** `/run/media/dusan/DATA1/aura-os` (the main worktree):
  Aura (another agent) is working there on task 001b, uncommitted.
- Put the foot fork in `term/` (git subtree of
  https://codeberg.org/dnkl/foot, latest release tag). Aura-specific
  code goes in new files (`term/aura-*.c/h`) with minimal hooks in
  foot's own sources, so upstream merges stay easy.

## Test VM (Arch + sway, shared with Aura)
- `ssh -i ~/.ssh/aura_vm_ed25519 -p 2222 dusan@localhost`
  (sudo needs no password in the guest only).
- Sync: `rsync -a --exclude .git /run/media/dusan/DATA1/aura-os-term/term/ dusan@localhost:aura-term/`
  (with `-e "ssh -i ~/.ssh/aura_vm_ed25519 -p 2222"`). Build in the
  guest with meson/ninja (install them with pacman in the guest;
  foot's build deps: see its PKGBUILD / meson.build).
- Install to `/usr/local` in the guest only.
- Screenshots of the VM: `/run/media/dusan/DATA1/aura-os/vm/vm-keys.py shot /tmp/x.png`
  then read the image; keys: `vm-keys.py key ctrl-shift-a`,
  `vm-keys.py type 'ls /nope' --enter`.
- **Do not** reboot the VM, restart greetd, or edit sway/greetd/
  waybar config — Aura owns system config right now. To make
  aura-term the Super+Return terminal, write the change as a patch
  file in `config/` for Aura to apply later; test by launching
  aura-term directly.
- The VM has 2.5 GB RAM and software rendering; don't run heavy
  parallel builds (`ninja -j2`).
- If the VM is down, tell Dusan; don't start it yourself (host RAM is
  tight — Chrome uses ~5 GB).

## Interfaces you must define (003 isn't built yet)
- **Agent status:** aura-term writes one JSON file per window to
  `$XDG_RUNTIME_DIR/aura-os/panes/<pid>.json`:
  `{"pid","cwd","branch","agent":"claude|codex|aura|null",
    "state":"idle|working|done|input","since":<unix ts>}`,
  written atomically (tmp + rename), only on change. Document it in
  `docs/pane-status.md`; task 003's sidebar will read it.
- **Ask Aura bridge:** `bin/aura-term-ask` reads one JSON object on
  stdin (`selection` or `command/output/exit/cwd`), talks to
  `aura sidecar`, prints Aura's answer. The terminal opens it in a new
  sway split (`swaymsg exec`). Aura is NOT installed in the VM: if
  `aura` is missing, the helper prints a clear stub message — build
  and test everything else, and test the real bridge on the host with
  a read-only call (no tools approved).

## Ask first
- Anything that changes the host (packages, /etc, services).
- Replacing foot as the default terminal system-wide.
- If the spec's "fork foot" approach hits a wall — say why before
  switching approaches.

## Report at the end (and after each milestone)
- What works, verified how (commands, screenshots); what's untested.
- RSS per idle window in server mode vs. upstream foot, and the
  latency comparison method + numbers.
- `git log --oneline` of your Aura-only commits vs. the foot base.
- Host vs guest for every file you wrote outside the worktree.

Milestones: (1) foot fork builds and runs as aura-term in the VM →
(2) command blocks (OSC 133) → (3) pane status files → (4) Ask Aura →
(5) budget measurements. Commit and report after each.
