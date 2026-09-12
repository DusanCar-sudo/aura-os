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
- Cost: 2.95M tokens (2.86M input) for ~460 lines of shell. Input
  dominates, so context is being re-sent heavily.
  → aura-code: check compaction and re-reading of files already read.
- Built from the pre-budget brief: kitty, SDDM (+Xorg 84 MB), and
  polkit-kde (Qt daemon). Not her fault (budget landed mid-task), but
  she did not re-read AURA.md before finishing.
  → aura-code: re-read AURA.md when it changes during a session.
- `.aura/token-log.jsonl` is tracked in the repo (session noise).

Measured after 001: VM idle 736 MB used. Hyprland 248, kitty 174,
Xorg 84, Xwayland 80, waybar 47, polkit-kde 39 (MB RSS).
