#pragma once

struct wl_window;

/* Tag the sway window "aura-pane-<shell pid>" (xdg-toplevel-tag), so the
 * aura-os-status sidebar can match $XDG_RUNTIME_DIR/aura-os/panes/<pid>.json
 * to its window via the "tag" field in swaymsg -t get_tree. A user-set
 * toplevel-tag wins. aura-termd only (needs wayland.c). */
void aura_window_set_pane_tag(struct wl_window *win);
