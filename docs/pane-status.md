# Pane status files (aura-term → aura-os-status)

aura-term reports what runs in each terminal window, for the task 003
sidebar ("spaces" list and agent status). The terminal is the writer;
the sidebar only reads.

## Where

    $XDG_RUNTIME_DIR/aura-os/panes/<pid>.json

One file per window. `<pid>` is the window's shell (the terminal's child
process), not aura-termd: in server mode every window shares one
aura-termd process. Directories are created mode 0700.

- Created when the window's shell starts, removed when the window closes.
- Written atomically (`<pid>.json.tmp` + `rename`), so a reader never sees
  a partial file.
- Written only when the content changes (state, agent, cwd, branch).
  Repeated OSC 7 reports of the same cwd do not touch the file.
- If aura-termd crashes, files can be left behind: treat a file whose
  `pid` is not alive (`kill -0`) as stale.

## Format

One JSON object, one line:

```json
{"pid":6531,"cwd":"/home/dusan/aura-os","branch":"main","agent":"claude","state":"input","since":1789229302}
```

| field    | type           | meaning |
|----------|----------------|---------|
| `pid`    | int            | shell pid; same as the file name |
| `cwd`    | string \| null | from OSC 7 (shell integration); before the first OSC 7, `/proc/<pid>/cwd` |
| `branch` | string \| null | git branch of `cwd` (read from `.git/HEAD`, worktrees included); short sha when detached; null outside a repo |
| `agent`  | `"claude"` \| `"codex"` \| `"aura"` \| null | agent running in the foreground; null at the shell prompt or for any other program |
| `state`  | `"idle"` \| `"working"` \| `"done"` \| `"input"` | see below |
| `since`  | int            | unix time of the last `state` change |

Sidebar symbols (task 005 spec): `working` ●, `done` ✓, `input` !,
`idle` nothing.

## Matching a file to its sway window

Each aura-term window sets its xdg-toplevel tag to `aura-pane-<pid>`
(sway ≥ 1.12 shows it as `"tag"` in `swaymsg -t get_tree`), so the sidebar
can find, focus or label the window for a pane file:

    swaymsg '[tag="aura-pane-6531"] focus'

A user-configured `toplevel-tag` in aura-term.ini overrides this; such
windows still have status files but no tag to match.

## States

Needs the shell integration (`config/shell/aura-term.bash` / `.zsh`,
OSC 133 + OSC 7). Without it a window stays `idle`.

| event | → state |
|-------|---------|
| window opens | `idle` |
| command starts (OSC 133;C) | `working` |
| command finishes (OSC 133;D, or a new prompt without D) after an agent ran, after ≥ 10 s, or while `done`/`input` | `done` |
| command finishes, anything else | `idle` |
| bell while an agent runs | `input`, plus a desktop notification (mako) "‹agent› needs input" |
| bell while a non-agent command runs | `done` |
| OSC 9 / 777 / 99 notification while a command runs, from an agent or with question-like text (permission, approve, allow, confirm, waiting, input, `?`) | `input` |
| OSC 9 / 777 / 99 notification while a command runs, otherwise | `done` |
| bell or notification at the prompt (e.g. tab completion) | no change |
| key press while `done` or `input` | `working` if a command still runs, else `idle` |

OSC notifications are already shown by aura-term itself (foot's
desktop-notifications), so only bell-triggered `input` sends the extra
notification. Desktop notifications use `[desktop-notifications]
command` (default `notify-send`, which needs libnotify installed for mako
to receive them) and are suppressed for the focused window unless
`inhibit-when-focused=no`.

The spec's check, `sleep 20; printf '\a'`: `working` at once, `done`
at the bell (and it stays `done` when the command finishes 20 s later).

## Agent detection

When a command starts, aura-term identifies the pty's foreground process
group (`tcgetpgrp`) after the next output, and on any bell or
notification — no polling. It matches `/proc/<pgid>/comm`, then
`argv[0]`/`argv[1]` of `/proc/<pgid>/cmdline` (for `node …/claude`):

- `claude`, or a path containing `claude-code` → `claude`
- `codex`, `codex.js`, or a path containing `@openai/codex` → `codex`
- `aura` → `aura`

An agent run inside ssh, tmux or a container is not detected; it counts
as an ordinary job.

## Reading it (sidebar)

Watch the directory with inotify: `IN_MOVED_TO` on `*.json` is an update
(every write is a rename), `IN_DELETE` on `*.json` is a closed window.
Ignore `*.json.tmp`. Re-read only the changed file; don't poll.
