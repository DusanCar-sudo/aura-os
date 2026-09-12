# Aura behaviour notes

Where Aura did well or stumbled while building Aura OS. Stumbles
become fixes in aura-code, not just in this repo.

## Task 001 — install skeleton (2026-09-12)

Good
- Scripts are idempotent, `set -euo pipefail`, snapshot-first, and
  report what changed (`comm` diff of `pacman -Qq`). Matches AURA.md.
- Stayed off the host: the SDDM autologin file lives in the guest only,
  and she explicitly refused to write host /etc.
- Verified in the VM (md5 of deployed vs repo file, re-run, remove and
  re-create) instead of trusting exit codes.
- Moved an inline heredoc into a tracked repo file when challenged.

Stumbles
- Misleading report: said "I added SDDM autologin" as if a host file
  had been written with write_file; corrected only after being asked.
  → aura-code: final reports should name host vs guest and the tool
    that actually wrote each file.
- Cost (corrected): tokens looked huge (11.5M input over 126 turns
  across 001+001b) but cache hit is 94% and cost $0 on DeepSeek flash.
  Re-sending context is normal; the real watch item is peak context
  (144k/turn) before compaction. Tag: aura (compaction threshold).
- Built from the pre-budget brief: kitty, SDDM (+Xorg 84 MB), and
  polkit-kde (Qt daemon). Not her fault (budget landed mid-task), but
  she did not re-read AURA.md before finishing.
  → aura-code: re-read AURA.md when it changes during a session.
- `.aura/token-log.jsonl` is tracked in the repo (session noise).

Measured after 001: VM idle 736 MB used. Hyprland 248, kitty 174,
Xorg 84, Xwayland 80, waybar 47, polkit-kde 39 (MB RSS).

## Task 001b — sway move (2026-09-12, in progress)

Good
- Removed Hyprland/kitty/SDDM/Xorg through a new snapshot-first
  `aura-os-remove` (snapshots 4 and 5). Idle RAM ~320 MB without apps.

Stumbles
- Kicked Dusan out of his session while he was using the VM: rebooted
  it at 21:00, then ran `sudo systemctl restart greetd` at 21:01:59 and
  21:02:21 — each restart kills the running sway session. No warning.
  → aura-code: before reboot/restarting a display manager in a shared
    VM, announce it and wait, or check `loginctl` for an active seat.
- Debugged with raw `sudo mv` / `sudo systemctl` over ssh instead of
  aura-os-* commands (AURA.md: no raw sudo for system changes).
- greetd config comment says aura-os-sway picks the pixman renderer,
  but `command = "sway"` — comment and config disagree, and she then
  moved /usr/local/bin/aura-os-sway away.
- Rollback test: used `snapper rollback` on archinstall's layout where
  the boot entry pins `subvol=@`, so a rollback may not take effect.
  She found the risk herself (man page, then fetched snapper source
  from GitHub) and took safety snapshot 7 first. Tag: brief — 001b told
  her to "roll back to base-clean" without saying how on this layout.

Verified by Claude at 22:58 (001b reported done)
- Good: hyprland/kitty/sddm/xorg/polkit-kde all removed; greetd +
  tuigreet, zram (1.2 GB), foot, sway 1.12 in place. aura-os-bench
  works and honestly prints "OVER BUDGET".
- Bench: 473 MB idle, but 141 MB of that is dolphin + kioworker
  (running 39 min, parent PID 1 — D-Bus activated, not in the plan;
  archinstall's Hyprland profile installed it). Without it ~330 MB:
  within budget.
- Repo and VM disagree: repo config/sway/config has `xwayland enable`
  (correct per brief), VM has `xwayland disable`; bin/aura-os-reset is
  in the repo but not installed in the VM. So the final repo state was
  never deployed — "done" was not verified on what's committed.
  Tag: aura — "done means tested" (AURA.md) not applied to the last
  edits. The fresh-from-base-clean run would have caught it.

## Task 003 — herdr desktop (2026-09-12, in progress)

Stumbles (found at 23:50, VM boot shows swaynag "errors in config")
- config.d/10-binds.conf: `bindsym --repeat` — no such flag in sway
  (4 resize binds dead). `titlebar_padding 0 4` rejected (Invalid size).
- A multi-line Python script pasted inline into the sway config
  (lines 65-69) — sway config has no multi-line exec; also breaks the
  "no Python in the core" budget rule.
- Never ran `sway -C` (config check) before deploying: a headless
  check (`WLR_BACKENDS=headless WLR_RENDERER=pixman sway -C`) catches
  all of these over ssh. Tag: aura — validate before deploy.

## Incidents in the shared VM (2026-09-13)
- ~00:17 the aura-term Claude session ran `pkill -x foot` twice,
  killing ~4 foot windows in the live sway session (Dusan's and the
  Aura prompt's). Self-reported; fixed to PID-scoped kills.
- ~/Aura-Pulse (Dusan's clone of dusancar-sudo/aura-pulse, created
  ~23:00) is gone from the VM. /home mtime is 00:15. No sudo rm in the
  journal, no rm in bash history, not in aura-term test scripts,
  aura-os-reset leaves /home alone. Cause not yet found — both agents
  asked to check their own transcripts for deletions under ~.
- Lesson for both: a shared VM needs a written "who owns what" rule;
  added to the brief for the next task.
- 00:50 Dusan: "it doesn't seem good" — Claude took over 003. Found:
  two waybar processes (double top bar + sidebar), literal `▸`
  (bash double quotes don't expand \u), tabs 6-9 always shown (spec:
  1-5 + "+"), agent states guessed from /proc state (T="input",
  Z="done" — wrong; aura-term pane files exist for this), ~30 process
  spawns per sway event (9 labels × swaymsg+jq), sidebar centered,
  black background (swaybg missing). Tag: aura — never looked at a
  screenshot of her own result (vm-keys.py shot was available).
