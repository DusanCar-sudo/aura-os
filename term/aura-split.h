#pragma once

#include <stdbool.h>

struct terminal;
struct wl_window;

enum aura_split_dir {
    AURA_SPLIT_NONE,
    AURA_SPLIT_LEFT,
    AURA_SPLIT_RIGHT,
    AURA_SPLIT_UP,
    AURA_SPLIT_DOWN,
};

/*
 * Splits (Ctrl+Alt+arrows): open a new aura-term pane left /
 * right / above / below this one, in this pane's working directory,
 * using sway's native splits (every pane stays a sway window, so pane
 * status and the sidebar see it). Outside sway: a plain new window.
 * aura-termd only.
 */
bool aura_split(struct terminal *term, enum aura_split_dir dir);

/* toplevel-tag "aura-split-left|up" marks a pane that must move itself
 * before its opener once mapped (sway only inserts after the focused
 * window). Returns true if tag was such a marker. */
bool aura_split_parse_tag(struct terminal *term, const char *tag);

/* After the window's first configure: do the pending move, if any */
void aura_split_first_configure(struct wl_window *win);
