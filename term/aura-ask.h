#pragma once

#include <stdbool.h>

struct terminal;

/*
 * Ctrl+Shift+A (aura-ask): send the selection, or else the last failed
 * command block, to Aura. Writes the request as JSON to
 * $XDG_RUNTIME_DIR/aura-os/ask/<pid>-<n>.json and opens aura-term-ask on
 * it in a new window next to this one (swaymsg exec; plain spawn outside
 * sway). Never writes into the running program. aura-termd only.
 */
bool aura_ask(struct terminal *term);
