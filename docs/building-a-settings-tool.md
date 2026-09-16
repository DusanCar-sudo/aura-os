# Building a settings tool

Every everyday setting in Aura OS is the same shape: **one vertical list you
arrow through or click, state on the left, no forms and no REPLs**. This file
is how to build the next one so it matches the ones that exist.

The rule behind the rule: *a person who does not know the command line must be
able to do the thing.* Dropping someone into `bluetoothctl` is not a settings
tool, it is a shell with extra steps. That is what these replaced.

## The shape

    [ ON  ]  Bluetooth
    ──────────────
    ●  Lenovo Wireless Stereo Headset
    ○  HONOR CHOICE Headphones Pro
    ⟳  Scan for new devices
    ←  Back

A row is one of three things, and never anything else:

| Row | Means | On pick |
|---|---|---|
| `[ ON  ] X` / `[ OFF ] X` | a toggle, current state shown | flip it, stay in the list |
| `●` connected · `○` known · `·` seen | an item with state | open its actions |
| `⟳ ★ ◈ ←` + words | an action | do it |

State goes **first** so the list answers "is it on?" without being read
left-to-right. Never a second column, a tab, or a grid.

## Skeleton

```bash
#!/usr/bin/env bash
# aura-os-thing — one sentence on what a person can do here.
set -uo pipefail
. "$(dirname "$(readlink -f "$0")")/aura-os-menu"

state() { probe somectl status | grep -q yes && echo on || echo off; }

while :; do
    s="$(state)"
    pick="$( { row_toggle "$s" "Thing"
               rule
               echo "⟳  Do something"
               row_back; } | menu "thing" )"
    case "${pick:-}" in
        "") exit 0 ;;                       # cancelled — always exit
        *"Thing") toggle_it ;;
        "⟳  Do something") do_it ;;
        "←  Back"|"──────────────") exit 0 ;;
    esac
done
```

`bin/aura-os-menu` is the whole library: `menu`, `probe`, `row_toggle`,
`row_back`, `rule`, `bar`, `notify`, `have`. Source it, use it, add to it
rather than growing a second one.

## Five things that will bite you

These are not style notes. Each one cost real debugging time, and each is
invisible until you run the tool on the actual machine.

1. **`--layer overlay` is mandatory.** sway stacks tiled and fullscreen
   windows above the layer-shell `top` layer, so a menu without it opens,
   accepts keystrokes, and draws *underneath the window* — it looks like the
   tool silently did nothing. `menu` already passes it; do not call `fuzzel`
   directly.

2. **Wrap every state probe in `probe`.** `bluetoothctl` blocks forever when
   no controller is present — absent hardware, or just rfkilled — and hangs
   the tool before it draws its first row. `probe` is a 3-second `timeout`.

3. **`local` expands all its arguments before assigning any.**
   `local p="$1" n=$((p*2))` sees an *unset* `p`. Split the lines.

4. **`cut -c` counts bytes, not characters.** It slices `▁▃▆█` in half and
   prints mojibake. Index an awk array instead. Related: mawk cannot put
   multibyte characters in a bracket class, so match structure
   (`/[A-Za-z]+: *$/`) rather than box-drawing glyphs.

5. **Do not carry your own palette.** `aura-os-theme` writes
   `~/.config/fuzzel/theme.ini`, so `fuzzel` is themed system-wide already.
   A tool that passes its own colors stops following the theme the moment
   someone switches it.

## Wiring a new tool in

1. `bin/aura-os-<thing>` — `install/04-aura.sh` picks up `bin/aura-os-*` by
   glob, so nothing to edit there.
2. Add a line to the `items` list in `bin/aura-os-settings` **and** a case
   branch. Keep it to one line: the picker does `${pick%% *}`, so a wrapped
   entry breaks.
3. A key binding goes in `config/sway/config.d/15-fkeys.conf`, and must be
   added to `install/03-sway.sh`, which lists config.d files explicitly.
4. Bind the bare key *and* the `XF86` code — the same physical key sends
   different codes depending on FnLock. Add `--locked` only for things that
   must work on the lock screen (sound, brightness).

## Testing it without rebooting the laptop

`tools/aura-vm` boots the installed system from its own partition in QEMU,
in snapshot mode, so the real install cannot be touched:

```sh
tools/aura-vm --fullscreen        # snapshot; writes are thrown away
tools/vm-type 'some command'      # type into it (no guest agent needed)
tools/vm-type --key ret
```

Two things about the VM worth knowing before you trust it: keystrokes must be
paced (~35ms), or virtio-keyboard silently drops and reorders them and you
debug a command you never actually typed; and the guest resolution follows
the QEMU window size, so `--fullscreen` is how you get a real one.

Fastest way to check a list reads well, without a GUI at all — stub the
picker and look at the rows:

```sh
sed 's|^\. "\$(dirname.*|. bin/aura-os-menu; menu() { cat; echo ""; }|' \
    bin/aura-os-thing | bash
```

## Adding a theme

`themes/<name>/palette`, 15 `key=#rrggbb` lines — copy an existing one. That
is the whole job: `aura-os-theme apply` recolors sway, waybar, aura-term,
fuzzel, GTK and Qt from it.
