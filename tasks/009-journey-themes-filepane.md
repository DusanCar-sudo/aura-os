# 009 — Learning journey, the data reset, TUI colors, right file pane

First written 2026-09-19 on the Ubuntu side as `tasks/008-journey-themes-filepane.md`
in the mirror (`/mnt/data/aura-os`, branch `icons-splash-handoff`). Moved
here, to the canonical repo, the same day on Aura OS and brought up to date.
The mirror copy is history now; this file is the one to follow.

**Approved by Dusan on 2026-09-19: all four parts, including every
recommendation.** Don't re-ask these. Only ask about what this file leaves
open (marked **open**).

Repos:
- Aura OS: this repo, `/mnt/bigdata/aura-os-work` (canonical, GitHub).
  `/mnt/data/aura-os` is a divergent mirror: read it, never push from it.
- Aura Code: `~/aura-code` (= `/mnt/bigdata/aura/aura-code`). Other
  sessions edit its `master` working tree (loop.ts, mcp.ts, env.ts, …),
  so commit narrowly and never stage files you didn't touch.

## Brought over from the mirror (2026-09-19)

- `assets/icons/*.svg`: 16 square-stroke icons. `aura-os-theme apply`
  writes recolored copies to `~/.local/share/aura-os/icons/` (fg color,
  `aura.svg` in accent). Tested with tokyo-night and ember.
  `install/04-aura.sh` installs the originals to `/usr/share/aura-os/icons`.
- Aura's own notifications carry the mark (`-i "$aura_icon"`):
  `aura-os-mcp` (notify tool), `aura-os-tab`, `aura-os-setup`, `aura-os-profile`.
- `tools/make-splash.py` variant `g-signal`, rendered to
  `assets/splash/aura-splash-signal.png`. **Not the active splash:** this
  repo already had a newer hand-made design (`628c62b1`), which stays in
  `aura-splash.png`. **Open:** Dusan picks which one boots. g-signal
  takes any theme (`v_signal(W, H, theme='tokyo-night')`); `aura` uses
  the built-in brand colors because there is no themes/aura here.
- `bin/aura-os-splash` + `config/systemd/aura-os-splash.service` +
  `docs/splash-loading-line.md`: the tty1 loading line before greetd,
  installed by `install/03-sway.sh`. Never deployed yet.
- Not ported on purpose: the mirror's themes/aura, neon and sunset (this
  repo has its own 16) and its popups (this repo has newer Rust ones).

Found while porting, not fixed (another session had `aura-os-theme` open):
on a fresh user with no `~/.config/sway/config.d/60-wallpaper.conf`,
`aura-os-theme apply` dies at the wallpaper step (`sed` on a missing file
under pipefail → exit 2) **before it saves the theme**. First-boot bug.

---

## Part 1 — Learning journey · BUILT 2026-09-19

Aura Code branch `learning-journey` (commits `9bd9b58`, `9b77e02`):
- `src/agent/learning-graph.ts`: `buildLearningGraph` (runs from
  `~/.aura/episodes/*/*.json`, memories from `memory/episodes.jsonl`,
  global + project lessons, eras from `~/.aura/archive/*/MANIFEST.json`)
  and `learningFrames` (size-aware text frames).
- `GET /api/learning/graph?days=&since=&until=` (tested live: 200 / 400 / 401).
- Protocol `learning.frames` (documented in `docs/PROTOCOL.md`).
- `:journey [days]` in the REPL (palette: "Learning journey") and
  `aura journey [days]`: chart + list, Enter opens one, `w` window
  7/30/90/365, `r` reload. Tested in a pty: standalone and inside the REPL
  (repaints the chat on close).
- Tests: `tests/agent/learning-graph.test.ts`, `tests/cli/journey-view.test.ts`,
  `learning.frames` cases in `tests/protocol/handler.test.ts`.

This repo: `bin/aura-os-journey` + `config/sway/config.d/80-journey.conf`
(Super+J → floating aura-term). It checks the installed aura-code has
`journey-view.js`; 0.18.0 doesn't, and would send "journey" to the model
as a task, so it shows "needs a newer Aura Code" instead.

Left: release aura-code (merge `learning-journey`, bump, publish), bump
`AURA_CODE_VERSION` in `install/06-aura-code.sh`, deploy Super+J live
(`~/.config/sway/config.d/80-journey.conf` + `aura-os-journey` on sway's
PATH — sway's PATH lacks `~/.local/bin`, use an absolute path), then a
screenshot on the laptop.

Real numbers (Aura OS `~/.aura`, 2026-09-19): 454 runs, 57% succeeded,
22 memories, 0 dated lessons. The "131 episodes, 11%" in
`lessons-global.md` is a stale digest; don't quote it.

---

## Part 2 — 10-day tracking window, then a documented reset

Dusan's decision: until now Aura was in deep development, mostly trial
and error: models swapped, prompts and tools rewritten, runs killed
mid-change. The numbers so far measure the building work, not Aura.

1. **Track 10 days**, 2026-09-20 → 2026-09-29, normal daily use. Watch
   it with `aura journey` / Super+J.
2. **Collect**: at the end, save `GET /api/learning/graph` JSON for both
   periods (the dev phase before 2026-09-20, and the 10-day window).
3. **Reset, never delete**: new `aura learning archive --label dev-phase`
   (Aura Code) moves `~/.aura/memory/{episodes.jsonl,lessons-global.md}`,
   the per-project `.aura/lessons.md` files and `~/.aura/episodes/` into
   `~/.aura/archive/2026-09-30-dev-phase/` with a `MANIFEST.json`
   (`label`, `archivedAt`, counts, date range, success rate, sha256 per
   file), then starts empty. Snapshot first (`aura-os-snapshot`).
   Idempotent, prints what it moved. Never touch `~/.aura/*.env`, tokens,
   keys or `secrets.json`, not even to archive them. Keep `identity.json`,
   `conversational.md`, `relationship.json` and `user*.json`: who Aura is
   and who Dusan is, not what she learned. **Open:** ask about anything else.
4. **Era marker**: already built. `buildLearningGraph` reads the manifest
   and the chart draws `|` on the reset day. Still to do: `aura journey
   --era dev-phase` to view an archived era.
5. **The document**: `docs/RESET-2026-09.md` in Aura Code, plus a short
   note in this repo's docs. Why (the dev phase was trial and error; the
   numbers measured churn), what was archived and where, before/after
   numbers from the manifest, what was kept, how to restore. Write it
   after the archive runs, with the real numbers.

---

## Part 3 — Aura Code TUI: colors only, combos + picker

Scope: **colors, color combos and a picker. No layout, spacing or
behavior changes.**

Current state: about 530 hardcoded `chalk.hex('#…')` calls across 31
files (chalk 4), about 12 distinct colors, a warm clay set: `#cc785c`
(118 uses), `#b15439` (96), `#5a9e6e` (95), `#8a7768` (82), `#d4903a`
(46), `#cc9e5c`, `#c8b5a0`, `#e8d5b7`, `#ede0cc`, `#4e3d30`, `#6ed0ea`,
`#5a4a3a`, `#4a5568`, `#eee8e2`, `#8a94a6`. The new `src/cli/journey-view.ts`
uses the same hexes; move it along with the rest.

1. `src/cli/palette.ts`: named roles (`accent`, `accentDeep`, `ok`, `err`,
   `warn`, `muted`, `dim`, `text`, `textBright`, `border`, `info`, …)
   returning chalk functions. Map each hex by where it's used.
2. Replace every `chalk.hex('#…')` with its role, file by file. The
   default must look **pixel-identical**: today's look becomes `clay`.
3. Combos: `clay` (default) plus every Aura OS theme. This repo has 16
   in `themes/*/palette` (cyan-grid, ember, forest, gruvbox,
   high-contrast, mono, neon-day, neon-night, nord, ocean-depth, paper,
   purplerain, retro-amber, sunset, tokyo-night, violet-dusk), all with
   the same 15 keys, so one key→role mapping covers them. Read them at
   runtime from `/usr/share/aura-os/themes` (don't copy them in), so a
   new OS theme shows up in Aura Code for free.
4. Picker: `:theme` overlay (the `tui.ts` overlay system); ↑/↓ previews
   live, Enter saves to the Aura Code config, Esc reverts. Plus
   `aura theme <name>`.
5. On Aura OS the default is `auto`: follow `~/.config/aura-os/theme`.
   Read at startup; no watcher, no polling.
6. Respect `NO_COLOR`.

---

## Part 4 — Right pane with a file tree (Aura OS)

**BUILT + DEPLOYED 2026-09-22 (commit bc023ce8).** `aura-os-files pane`
(single-pane mode), `bin/aura-os-filepane-toggle` (scratchpad toggle,
floating at 22% width on the focused output so it never reshuffles the
tiled layout), `config/sway/config.d/82-filepane.conf` — **Super+G**
(Super+F is fullscreen; G is the nearest free key). 03-sway.sh now also
deploys 80-journey.conf, which until now was never installed. Verified
live on the laptop: pane shows ~/Documents over Chrome, toggle round-trips
to the scratchpad, pty tests for pane and two-pane modes pass. RAM cost
~2 MB (aura-term client; aura-termd was already running). Not done: open
in the focused window's directory (opens in $HOME; AURA_FILEPANE_DIR
overrides). "Reuse aura-files-rs vs separate tree" is settled: separate,
the bash listing — aura-files-rs stays the full-window file manager.

Decided: runs in **aura-term** (not a plain terminal).

Since the first handoff this repo gained `aura-files-rs` (`0b31a0c3`, the
file manager as a real window) and a draft two-pane `bin/aura-os-files`
(uncommitted, another session). **Open:** reuse one of those for the pane,
or keep a separate tiny tree. Ask Dusan once, showing both.

Sketch:
- `aura-os-files pane` (or a new module): toggles an `aura-term
  --app-id=aura-files-pane` on the right at about 22% width.
- sway: `for_window [app_id="aura-files-pane"]` placement. **Open:**
  per-tab vs one global pane. Global is simpler and uses less RAM.
- Bind: `Super+E` is taken; pick a free key (`J` is now the journey).
- Opens in the focused window's directory if one can be found, else `$HOME`.
- Report idle RAM before and after (budget: under 350 MB idle).

---

## Done means

It runs on the live laptop after a real reboot, confirmed with a
screenshot. Built-but-not-deployed doesn't count. Say what was verified
and what wasn't.
