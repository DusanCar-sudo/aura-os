# Task 001b — meet the speed and RAM budget

AURA.md gained a "Speed and RAM budget" section after you started 001.
Re-read AURA.md now. Your 001 desktop idles at 736 MB; budget is 350.

Measured RSS after 001: Hyprland 248, kitty 174, Xorg 84 (SDDM),
Xwayland 80, waybar 47, polkit-kde 39 (MB).

## Do
- kitty → foot (update aura-os-prompt: it checks KITTY_WINDOW_ID).
- SDDM → greetd + tuigreet, with autologin kept for the VM only.
  Remove the SDDM config file and its install step.
- polkit-kde → the lightest agent that works, or start one on demand;
  measure and justify.
- Xwayland off by default (`xwayland { enabled = false }`) unless
  something in the core needs it.
- zram swap (zram-generator).
- Add `bin/aura-os-bench`: prints idle RAM (after 30 s settle) and
  top 8 processes by RSS. Run it before and after, put both in your
  report.

## Rules
- Do this from the `base-clean` snapshot in a fresh run of install.sh
  too, not only on top of the current VM state.
- Report host vs guest for every file you wrote.
- Note: the VM draws with llvmpipe (software), which inflates
  Hyprland's RSS. Report it, don't chase it.
