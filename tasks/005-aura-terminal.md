# Task 005 — aura-term: Aura's own native terminal

Depends on 003 (herdr theme). Aura OS gets its own terminal, `aura-term`,
replacing foot as the default. It is a **fork of foot** (C, Wayland-
native, CPU-rendered): VT parsing, fonts, Unicode and IME are solved
there — our work is what makes it Aura's.

## Base
- Fork foot into `term/` (git subtree or submodule; keep upstream
  mergeable: Aura changes in separate files/commits, minimal diffs
  to foot's own sources). Binary `aura-term`, server `aura-termd`.
- Server mode by default: one process for all windows (RAM budget).
- herdr look from themes/herdr: square, 1px, monospace, no padding.

## What makes it Aura's
1. **Command blocks** — shell integration via OSC 133 (bash + zsh
   snippets shipped in config/). Each command is a block with exit
   code and duration shown in the margin; Ctrl+Shift+↑/↓ jumps
   between blocks; Ctrl+Shift+Y copies the last block's output.
2. **Ask Aura** — Ctrl+Shift+A sends the selection (or the last
   failed block: command + output + exit code + cwd) to Aura. Her
   answer opens in a split next to the terminal (sway), never inside
   the running program. A suggested command is shown, never run
   without Enter.
3. **Agent awareness** — the terminal knows what runs inside it:
   claude / codex / aura / any long job. It reports to
   `aura-os-status` (task 003 sidebar): working ● / done ✓ / needs
   input ! (detect via OSC 9/777 notifications, bell, and idle
   prompt after a job). Needs-input also sends a mako notification.
4. **Tab identity** — each window reports cwd + git branch to
   aura-os-status (for the "spaces" list) via OSC 7.

## Budget
- Idle window ≤ 15 MB extra RSS in server mode; key-to-glyph latency
  no worse than upstream foot (measure with `typometer` or a
  scripted input test; report both).
- No Python/Electron helpers; the Aura bridge is a small C or shell
  helper talking to Aura over her existing sidecar (`aura sidecar`,
  NDJSON over stdio — see aura-code docs/PROTOCOL.md).

## Done when (in the VM)
- aura-term is the Super+Return terminal; foot uninstalled.
- A failing `ls /nope` → Ctrl+Shift+A → Aura's explanation in a split.
- `sleep 20; printf '\a'` in one window shows ● then ✓ in the sidebar.
- Upstream foot merge still applies cleanly (report `git log` of the
  Aura-only commits).
