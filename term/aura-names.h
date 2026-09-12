#pragma once

/*
 * Aura OS names for the foot fork. Every place where upstream foot
 * hardcodes "foot" for a user-visible name, path or socket includes
 * this header instead, so upstream merges touch one-line hunks only.
 */

#define AURA_TERM_NAME        "aura-term"   /* client binary, app-id, title */
#define AURA_TERM_SERVER_NAME "aura-termd"  /* server binary */

/* Config: $XDG_CONFIG_HOME/aura-term/aura-term.ini, /etc/xdg/aura-term/... */
#define AURA_TERM_CONF_SUBPATH "/aura-term/aura-term.ini"

/* Server socket: $XDG_RUNTIME_DIR/aura-term-$WAYLAND_DISPLAY.sock,
 * with $XDG_RUNTIME_DIR/aura-term.sock and /tmp/aura-term.sock as fallbacks */
#define AURA_TERM_SOCK_PREFIX "aura-term"
