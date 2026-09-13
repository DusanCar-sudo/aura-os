# Claude's working memory for Aura OS

Copied from Claude Code's memory on 2026-09-13. Rules Claude follows while working on this repo.

## aura-os-project

Aura OS lives at `/run/media/dusan/DATA1/aura-os` (started 2026-09-12),
pushed to PRIVATE github.com/DusanCar-sudo/aura-os (main). Before any public release: rewrite old history (a deleted
reference image is still in it; needs force-push) and consider a noreply email. Aura OS is PROPRIETARY (not MIT — Dusan said so)
and ships to users ONLY as a downloadable ISO; LICENSE +
THIRD-PARTY-NOTICES.md hold the GPL-source / foot-MIT obligations.
Idea (Dusan's words): "finally your own OS on your own laptop, running what
you need", "following instincts and trends". Never name other distros or
tools that were only used as examples. Arch + **sway** (Dusan chose sway over Hyprland), look: square corners,
tabs 1-5 renamable + per-tab color (tab only), spaces/agents sidebar,
<350 MB idle, must still run all his apps/games/multimedia.

**Why:** Dusan explicitly wants this as a double project — Claude writes
task briefs, Aura builds them, Claude watches how she works and turns
her stumbles into aura-code fixes.

**How to apply:**
- Loop is in METHOD.md: brief → Aura runs → `tools/aura-trace.py`
  → verify in VM myself → NOTES-aura-behaviour.md (tag brief/rules/aura)
  → fix → re-run same brief from snapshot.
- VM: `vm/run-vm.sh` (QEMU, no GL so `vm/vm-keys.py shot` works),
  ssh `-i ~/.ssh/aura_vm_ed25519 -p 2222 dusan@localhost`, NOPASSWD sudo
  in guest only. Ctrl+Alt+Q quits QEMU (Dusan hit it by accident).
- Aura doesn't commit her own work (Dusan's rule); AURA.md near its
  2000-char cap — put new detail in task briefs.
- Task order: 001 ✓ → 001b (sway+budget) → 003 desktop UI → 004 daily
  driver → 002 voice.

Related: [[aura-md-convention]], [[aura-surface-confusion]]

## shared-vm-no-pkill-by-name

Never run `pkill -x foot` (or any pkill/killall by name) in the Aura OS VM
(`ssh -p 2222 dusan@localhost`). The VM runs Aura's real sway session at the
same time as my tests; on 2026-09-13 a `pkill -x foot` in a measurement
command killed ~4 foot windows in Aura's session. `pgrep -x NAME` also
silently counts her processes (it inflated a memory measurement to 140 MB).

**Why:** the VM is shared ([[aura-os-project]]); Aura works in it while I
test, and her windows use the same binaries as my tests.

**How to apply:** kill only PIDs I started (`$!`), or processes whose
`/proc/<pid>/environ` has my private `XDG_RUNTIME_DIR` — aura-os-term's
`tools/aura-term-test/hl.sh kill NAME...` does exactly that. Count memory
the same way. Run tests in the private headless sway, never the real one.

## vm-screenshot-privacy

When a VM screenshot shows Dusan on a sign-in, account or password page
(e.g. Google sign-in in Firefox inside the Aura OS VM), stop taking
screenshots until he says he's done, and don't comment on what's shown
(account names, emails).

**Why:** He called this "good instinct" (2026-09-13) after I paused
screenshots when a Google sign-in with his account list appeared.

**How to apply:** Verify changes by other means meanwhile (ssh, config
checks) and tell him what to look at himself. Related: [[aura-os-project]]

## aura-md-convention

`AURA.md` at the repo root is the standing-rules convention across all of
Dusan's repos — not `CLAUDE.md`, not `AGENTS.md`. On 2026-07-29 all 12 public
repos got one, and `CLAUDE.md` was deleted from `enables` and `agentmesh` to
consolidate.

Hard technical constraints, verified in the installed `aura-code` package at
`dist/agent/context.js:52`:

```js
auraRules: readTruncated(root, ['AURA.md'], 2000, 'AURA.md'),
```

- Aura loads **only** `AURA.md`, **only** from the repo root. No subdirectory
  lookup, no `CLAUDE.md`/`AGENTS.md` fallback.
- Content is **silently truncated at 2000 characters** (`slice(0, 2000)`).
  Anything past that is dropped with a `[...truncated]` marker. Keep every
  `AURA.md` under 2000 chars or the tail is dead weight.
- It is injected into the system prompt under the heading
  `### Aura Standing Rules`.

House format (from `aura-code/AURA.md`): `# Aura Standing Rules` H1, `## Topic`
sections, hard-wrapped prose paragraphs (~65 cols), imperative and firm —
"violations should be flagged as bugs, not treated as style preferences".
Prose, not tables.

**Why:** Dusan asked for this explicitly after a `CLAUDE.md` sitting in
`enables` turned out never to have been read by Aura at all.

**How to apply:** When adding project rules for Dusan, write `AURA.md`, check
`wc -c` is under 2000, and match the prose format. See
[[aura-surface-confusion]] for what these files exist to prevent.

## aura-surface-confusion

The recurring failure across Dusan's repos: a single git tree holds several
public "surfaces" that agents conflate. In `enables`, `index.html` is the
website (enables.vercel.app) while `README.md` is the GitHub repo page — so
"put this image in the repo" is ambiguous, and Aura put a screenshot on the
landing page instead of the README.

Cost of one instance (2026-07-29): 100 turns, 126 tool calls, 2.54M tokens,
and Dusan repeating himself 7 times. Commits `b19c84d6` (wrong) →
`d3852451` + `5b9d08b6` (undo).

Worst offenders:
- `lean-progress-iq` — 8 HTML files, only `index.html` is live; the other 7
  (`1index.html`, `ind34ex.html`, `milutinindex.html`, …) are dead drafts
  still publicly reachable because `cleanUrls` is on.
- `aura-pulse` / `ruby-diamond-client` — `index.html` is a Tauri **app shell**,
  not a website. The Aura Pulse website is a *separate repo*
  (`aura-pulse-website`).
- `aura-code` — the website is `site/` only (`vercel.json` sets
  `outputDirectory`); stray root-level HTML is served to nobody.

**Why:** Dusan's frustration here is about being misunderstood repeatedly, not
about code quality. Getting the surface right matters more than elegance.

**How to apply:** Before editing any image, copy, or layout in his repos, name
which surface the request means and say which file you changed — never just
"done". If two surfaces could match, ask one short question. This is what the
[[aura-md-convention]] files encode.
