# Method — building Aura OS to perfect Aura

Every task is an experiment: we know exactly what Aura was told, so
the gap between the brief and the result is Aura's gap, not ours.

## Loop per task
1. **Brief** — `tasks/NNN-*.md` (Claude writes it). Frozen once Aura
   starts; changes mid-task are logged as such (they confound results).
2. **Run** — Dusan pastes one line into `aura --interactive`.
3. **Watch** — `tools/aura-trace.py --facts 40` during and after:
   turns, peak context, cache %, and flags (reboots, raw sudo, host
   paths, web fetches, errors/retries).
4. **Verify** — Claude checks the result in the VM independently
   (ssh, `aura-os-bench`, screenshots via `vm/vm-keys.py shot`), never
   from Aura's report alone.
5. **Score** — score the run: Good / Stumbles,
   each stumble tagged with its root cause:
   - `brief`  the brief was unclear or changed → fix the brief
   - `rules`  AURA.md missing a rule → fix AURA.md (2000-char cap)
   - `aura`   Aura behaved badly despite clear instructions → fix
              aura-code (system prompt, tools, compaction, reporting)
6. **Fix** — `aura` fixes go into aura-code as small, separate
   commits that reference the note.
7. **Re-run** — roll the VM back to the task's start snapshot and give
   Aura the SAME brief with the fixed aura-code. Compare trace + result.
   Only a better re-run counts as a fix.

## Scorecard columns (per run)
task · aura-code commit · turns · peak context · cost · rule
violations · Dusan interrupted? · done-when checks passed · idle RAM
