# Task 003 — herdr-style desktop

Depends on task 001. Reference: herdr (the terminal multiplexer Dusan
uses for agents). The whole desktop must look and behave like herdr —
flat text panes, no decoration. Screenshot: tasks/ref/herdr.png.

## Look — "no graphics"
- Square 90° corners everywhere — windows, bars, tabs, launcher,
  notifications. Plain sway, never SwayFX (it adds rounding).
- Solid dark background, no wallpaper image. No blur, shadows,
  animations or gaps larger than 2px.
- 1px borders: active = purple (#8b7cf6-ish), inactive = dim gray.
- One monospace font everywhere (JetBrains Mono / Iosevka), incl. bars.
- Pane title drawn on the top border, left-aligned, like `─ claude ─`.
- Palette lives in themes/herdr/ so other themes can swap it.

## Layout behaviour
- Tiling with explicit split direction: sway native splits:
  `splith` / `splitv` then focus the new side. Keys:
  Super+Alt+←→↑↓ = next window opens left/right/up/down of focused.
- Resize any direction: Super+Ctrl+←→↑↓ (repeatable) and Super+drag.
- Focus: Super+←→↑↓. Move pane: Super+Shift+←→↑↓.
- Tabs inside a pane: sway `layout tabbed`, Super+T toggles tabbed/split.

## Top bar — desktops as tabs
- Waybar across the top, thin, only: `1 2 3 +`.
- Each workspace is a tab; active tab highlighted purple.
- Click `+` or Super+N → new empty workspace, switch to it.
- Super+1..9 switch, Super+W closes an empty workspace's tab.

## Left sidebar — spaces / agents
- Narrow Waybar on the left (not eww: too heavy), like herdr's panel:
  `spaces`: each workspace → project folder + git branch of its
  focused terminal.
  `agents`: running agents (aura, claude, codex) with status
  ✓ done / ● working / ! needs input, click to focus that pane.
- Data comes from `bin/aura-os-status --json` (Aura writes it; push updates via `swaymsg -t subscribe` events, no
  polling). Super+B hides/shows the sidebar.

## Done when
In the VM: open 4 terminals in a 2x2 herdr-like grid using only the
split keys, resize the top-left pane, create tab 2 with `+`, and the
sidebar shows both spaces. Screenshot it (vm/vm-keys.py shot) and
compare side by side with tasks/ref/herdr.png.
