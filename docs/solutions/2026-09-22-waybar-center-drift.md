# Waybar center block drifting off-centre on the laptop panel

**Date:** 2026-09-22 · **Machine:** Aura laptop (HDMI-A-1 4K @1.5, eDP-1 1920x1200 @1.0)

## Symptom

The quick-toggle group (`modules-center`) sat ~192 px right of centre on
eDP-1 while the identical bar on HDMI-A-1 was pixel-perfect. `fixed-center:
true` was set; restarting, a dedicated eDP-only instance and an explicit
`width` all changed nothing.

## Root cause

GTK3 `GtkBox` centers the center widget **unless the start children's
minimum size would overlap it** — then it pushes it right (this is the
"can still be pushed" of waybar's `fixed-center` docs). The pusher was the
invisible window title: `#window { min-width: 360px }` plus its natural
text (up to `max-length` 60 chars) made the left box's *requested* minimum
~1050 px, wider than the true-centre position, so the toggles got shoved.

The 4K bar survived because at scale 1.5 the left box's request, measured
against its wider allocation, stayed just under the threshold.

## Fix

Give the title zero footprint until hover (`config/waybar/style.css`):

```css
#window { color: transparent; padding: 0; min-width: 0; font-size: 0; }
#window:hover { color: @fg; padding: 0 16px; min-width: 360px; font-size: 13px; }
```

Design intent kept (hover the top-left to see the focused window's title);
on hover the bar re-flows for a moment, which is invisible in practice.

Gotcha: the per-output `top-edp` duplicate rules block at the end of the
stylesheet must carry the same fix — later rules override earlier ones.

## Verification

`grim -o eDP-1` + column-brightness grouping: toggle group mid went
1152.5 → 961.5 (true centre 960); HDMI stays 1922.5 (true 1920).
