# aura-os-splash — the real boot loading line

2026-09-15. The boot-screen mock's loading bar (tools/make-splash.py
`v_chrome`, "Loading agents ...") was a picture; the UKI splash is a
static BMP and can never animate. The real line lives one stage later:
`bin/aura-os-splash`, a systemd oneshot on tty1 ordered
`Before=greetd.service` (unit: `config/systemd/aura-os-splash.service`,
deployed by install/03-sway.sh).

## The trap: waiting on multi-user.target

First version waited for `multi-user.target` as its last milestone.
That deadlocks boot: greetd is WantedBy multi-user AND ordered after
the splash, so multi-user can never complete while the splash waits
for it — "A start job is running … (48s / 2min 1s)" until the 120 s
TimeoutSec. Rule: an ordered-before-greetd oneshot must never wait on
anything greetd is part of. It now watches sysinit.target +
basic.target, then glides to full over ~2 s and exits.

## Notes

- The bar fills 0→33→66% on the two real milestones, then a short
  settle run; `SECONDS+110` deadline + unit TimeoutSec keep it from
  ever blocking a stalled boot.
- systemd's own "OK Started …" console lines print over/under the
  splash line — suppressing them needs `quiet loglevel=` kernel opts
  (08-boot.sh territory), not done yet.
- Term colors: `themes/aura/palette` is the brand sheet (Charcoal
  #0B0C0E, Off White #EDE9E0, Silver #A7A9AC, Electric Teal #00E5D0,
  Soft Magenta #FF6AC7, Lime #C6FF4D); `aura-os-theme apply aura`.
  Same day: added `neon` (blue+pink) and `sunset` (pink red/orange/blue)
  combos, and file-type colors — aura-os-theme writes
  `~/.config/aura-os/ls-colors` (ANSI slots, so they follow the theme),
  05-aura-term exports it as LS_COLORS from .bashrc: .md teal, .json
  blue, .toml/.ini yellow, code green, images magenta, archives red,
  .log dim.
- Verified in the VM: theme live in aura-term, splash shows + exits
  cleanly (boot 1.2s kernel + 4.8s userspace, greetd starts the second
  the splash exits), idle RAM unchanged (splash is not resident).
