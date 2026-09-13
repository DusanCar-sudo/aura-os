# aura-term

Aura OS's terminal (task 005): a fork of [foot](https://codeberg.org/dnkl/foot)
1.28.0 in `term/`, running in server mode — one `aura-termd` process for
all windows, `aura-term` is the ~2 MB per-window client. Everything foot
does (VT, fonts, IME, Unicode, sixel, URL mode, search) works unchanged;
Aura's code lives in `term/aura-*.c` with small hooks in foot's sources,
so upstream foot merges apply cleanly (checked against foot master).

## Keys

| Key | Action |
|-----|--------|
| Ctrl+Shift+↑ / ↓ | jump to the previous / next command block (also foot's Ctrl+Shift+Z / X) |
| Ctrl+Shift+Y | copy the last command's output |
| Ctrl+Shift+A | Ask Aura about the selection, or the last failed command |
| Ctrl+Alt+← / → / ↑ / ↓ | split: new pane left / right / above / below, in this pane's directory |
| Ctrl+Shift+C / V, Ctrl+Shift+R, Ctrl+Shift+N, Ctrl+Shift+O, Ctrl+Shift+U | foot defaults: copy / paste, search, new window, URL mode, Unicode input |

Rebind in `~/.config/aura-term/aura-term.ini`, `[key-bindings]` section
(same format as foot.ini; Aura's actions are `aura-copy-block`,
`aura-ask` and `aura-split-left|right|up|down`).

## Features

- **Command blocks.** With the shell integration, every command is a
  block: exit code and duration are drawn at the right edge of the
  command's line (`✓ 1.2s` dim, `✗ 2 · 1.2s` red), only over blank
  cells. Blocks can be jumped between and copied.
- **Ask Aura.** Ctrl+Shift+A sends the selection or the last failed block
  (command, output, exit code, cwd) to Aura; her answer opens as a split
  next to the terminal, never in the running program. A suggested command
  runs only when you press Enter in that window. See
  [ask-aura.md](ask-aura.md).
- **Agent awareness and tab identity.** Each window keeps
  `$XDG_RUNTIME_DIR/aura-os/panes/<pid>.json` up to date — cwd, git
  branch, agent (claude / codex / aura) and state (idle / working ● /
  done ✓ / needs input !) — for the aura-os-status sidebar, and tags its
  sway window `aura-pane-<pid>`. Needs-input also sends a desktop
  notification. See [pane-status.md](pane-status.md).
- **Splits.** Ctrl+Alt+arrows open a new pane next to the
  current one in its working directory, using sway's native splits
  (`split h|v` + `exec aura-term`): every pane is a real sway window, so
  focus/resize/move keys, pane status and the sidebar all work on it.
  Sway only inserts after the focused window, so a left/up pane is
  opened with the marker tag `aura-split-left|up` and moves itself
  (`[tag="aura-pane-<pid>"] move left|up`) once mapped; it still gets
  its normal `aura-pane-<pid>` tag. Outside sway: a new window.
- **Own terminfo.** `TERM=foot`, with foot 1.28's own entry installed in
  `/usr/local/share/aura-term/terminfo` and exported as `$TERMINFO`;
  anything that loses `$TERMINFO` (sudo, ssh) falls back to ncurses' own
  `foot` entry.

## Shell integration

Command blocks, the pane cwd/branch and the failed block for Ask Aura need
it. Add to `~/.bashrc` (bash ≥ 5.1) or `~/.zshrc`:

    . /usr/local/share/aura-term/shell/aura-term.bash   # or aura-term.zsh

It emits OSC 133 (A/B/C/D;exit) and OSC 7; other terminals ignore them.

## Files

| Path | What |
|------|------|
| `~/.config/aura-term/aura-term.ini` | user config (foot.ini format) |
| `$XDG_RUNTIME_DIR/aura-term-$WAYLAND_DISPLAY.sock` | server socket |
| `$XDG_RUNTIME_DIR/aura-os/panes/<pid>.json` | pane status (read by the sidebar) |
| `$XDG_RUNTIME_DIR/aura-os/ask/` | Ask Aura requests (0600, deleted after use), sidecar log |

## Install (Aura OS, inside the system — never on the host)

Per AURA.md these belong in an `install/` step / `aura-os-*` command
(snapshot first); the commands are:

    # build deps
    pacman -S --needed meson ninja gcc pkgconf python tllist libutf8proc fcft \
        pixman wayland wayland-protocols libxkbcommon fontconfig ncurses sway

    # release build, the same recipe as Arch's foot (PGO + LTO, makepkg flags)
    tools/aura-term-build.sh pgo term term/build-pgo
    sudo ninja -C term/build-pgo install          # everything under /usr/local

    # shell integration
    sudo install -Dm644 -t /usr/local/share/aura-term/shell config/shell/aura-term.bash config/shell/aura-term.zsh
    echo '. /usr/local/share/aura-term/shell/aura-term.bash' >> ~/.bashrc

    # sway: $term = aura-term, autostart aura-termd, Ask Aura split rule
    git apply config/aura-term-sway.patch         # then redeploy + swaymsg reload

    # mako notifications (foot's default notify command is notify-send)
    pacman -S --needed libnotify

    # then foot can go; its terminfo is owned by ncurses and stays
    pacman -R foot

`tools/aura-term-build.sh dev term term/build` is the quick non-PGO build.
Tests (private headless sway, never the real session):
`tools/aura-term-test/`.

## Budget (task 005)

Measured in the VM, private headless sway, Arch's foot 1.28.0 (+pgo) vs
aura-term (+pgo, same recipe), both `--config=/dev/null`
(`tools/aura-term-test/measure-*.sh`).

| | foot | aura-term |
|---|---|---|
| server, no windows | 9.9 MB RSS / 2.8 MB PSS | 9.8 MB / 2.7 MB |
| first window (fonts load once), 1280×720 | +9.2 MB server + 1.8 MB client | same |
| first window, 1920×1080 | +18.5 MB + 1.8 MB | +18.4 MB + 1.8 MB |
| each further window, tiled on one screen | ~+0.2 MB server + 1.8 MB client | same |
| each further full-size window (own workspace), 1280×720 | +7.6 MB + 1.8 MB | same |
| each further full-size window (own workspace), 1920×1080 | +16.9 MB + 1.8 MB | same |
| idle CPU, 5 windows, 10 s | 0 ticks | 0 ticks |
| key → frame, `cat` echo (800 samples) | median 0.81 ms, p95 0.93 | median 0.81 ms, p95 0.94 |
| key → frame, `bash` echo (800 samples) | median 0.91 ms, p95 1.07 | median 0.92 ms, p95 1.05 |
| launch → window mapped (40) | median 7.8 ms | median 7.5 ms |

The ≤ 15 MB per idle window budget holds for tiled windows and for
full-size windows up to 1280×720; a full-size 1920×1080 window on its own
workspace costs ~18.7 MB, exactly as in upstream foot: the server keeps
two screen-sized shm buffers per window, and sway 1.12 does not send the
xdg-toplevel `suspended` state to windows on hidden workspaces, so they
cannot be dropped there. For comparison, standalone foot (today's
`$term`) costs 28 MB for the first window and +11.6 MB for each further
tiled window at 1920×1080.

Latency: key → frame is from the key event reaching the terminal to its
`wl_surface.commit` of the frame showing the glyph (WAYLAND_DEBUG
timestamps); compositor scan-out is excluded and identical for both.
No typometer (needs a real display and camera) was run.
