# Task 001b — move to sway, meet the speed and RAM budget

Two changes landed after you started 001. Re-read AURA.md now.
1. Dusan chose **sway** over Hyprland: native left/right/up/down
   splits, tabbed containers and title bars fit the herdr look
   (task 003), at about half the RAM.
2. AURA.md has a "Speed and RAM budget": idle under 350 MB.
   Your 001 desktop idles at 736 MB.

Measured RSS after 001: Hyprland 248, kitty 174, Xorg 84 (SDDM),
Xwayland 80, waybar 47, polkit-kde 39 (MB).

## Do
- Hyprland → sway. Replace install/03-hyprland.sh with 03-sway.sh
  and config/hypr/ with config/sway/config (+ config.d/). Same keys:
  Super+Return terminal, Super+D fuzzel, Super+A Aura prompt.
  Waybar uses the sway/workspaces module.
- kitty → foot (aura-os-prompt checks KITTY_WINDOW_ID; fix it).
- SDDM → greetd + tuigreet, autologin into sway for the VM only.
  Remove config/sddm/ and its install step.
- polkit-kde → the lightest agent that works, or on demand;
  measure and justify.
- Xwayland stays on but lazy (`xwayland enable`: starts only when an
  X11 app opens). Many of Dusan's apps need it — never disable it.
- zram swap (zram-generator).
- Uninstall what you replaced (hyprland, kitty, sddm, polkit-kde)
  through a snapshot-first aura-os-* command.
- Add `bin/aura-os-bench`: idle RAM after a 30 s settle, plus the
  top 8 processes by RSS. Run it before and after; both go in the
  report.

## Rules
- Also prove it from scratch: roll the VM back to snapper snapshot
  1 "base-clean" and run install.sh end to end.
- Report host vs guest for every file you wrote.
- The VM draws with llvmpipe (software), which inflates the
  compositor's RSS. Report it, don't chase it.
