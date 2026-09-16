# Popups: click to open, stay open, click outside to close

2026-09-15. Popups used to close the moment the mouse left them — sway's
default `focus_follows_mouse yes` gives focus to whatever the mouse
hovers, and fuzzel + the floating TUIs close on focus loss.

## The fix (three parts)

1. `config/sway/config`: `focus_follows_mouse no` — hovering never
   steals focus, so a popup keeps it while the mouse wanders. Clicking
   elsewhere still focuses (clicks always did).
2. `bin/aura-os-popup` — wrapper for the floating TUIs (calendar,
   wiremix, nmtui/replaced menus, hold-screens). Spawns an aura-float
   term and watches sway's `window` events (subscribe — nothing polls):
   once the popup had focus, the first focus event naming another window
   is a click outside → it closes. One second of grace at spawn.
3. `bin/aura-os-wifi` / `bin/aura-os-bluetooth` — fuzzel menus with
   toggles replacing raw nmtui/bluetoothctl: wifi (turn on/off, scan +
   connect with password prompt, known networks connect/forget,
   disconnect) and bluetooth (power on/off, 10 s scan + pair + trust +
   connect, connect/disconnect/remove). `aura-os-settings wifi /
   bluetooth` open them; both also take `toggle` for a key binding.

fuzzel menus (clip, recent, settings list, theme pick) already close on
focus loss — with `focus_follows_mouse no` that now means click-outside.

## Verified in the VM

- Calendar popup open → mouse moved away → still open → click outside →
  closed (screenshots popup-1/popup-2 sequence).
- wifi menu renders status + "(no wifi device…)" on the ethernet-only
  VM; bluetooth menu branch for "controller off" is code-reviewed only —
  the VM has no bt controller, untested end to end.
