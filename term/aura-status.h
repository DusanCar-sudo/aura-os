#pragma once

/*
 * Aura pane status: one JSON file per terminal window at
 * $XDG_RUNTIME_DIR/aura-os/panes/<shell pid>.json, read by the
 * aura-os-status sidebar (task 003). Format and state machine:
 * docs/pane-status.md in the aura-os repo.
 *
 * Kept free of terminal.h so terminal.h can embed struct aura_status.
 */

#include <stdbool.h>
#include <stdint.h>
#include <time.h>

struct terminal;

enum aura_pane_state {
    AURA_PANE_IDLE,
    AURA_PANE_WORKING,
    AURA_PANE_DONE,
    AURA_PANE_INPUT,
};

enum aura_agent {
    AURA_AGENT_NONE,
    AURA_AGENT_CLAUDE,
    AURA_AGENT_CODEX,
    AURA_AGENT_AURA,
};

struct aura_status {
    uint8_t state;          /* enum aura_pane_state */
    uint8_t agent;          /* enum aura_agent */
    bool classify_pending;  /* foreground program not identified yet */
    time_t since;
    char *branch;
    char *path;             /* status file, NULL = disabled */
    char *last_json;        /* last written content, to write only on change */
};

void aura_status_init(struct terminal *term);     /* after the shell is spawned */
void aura_status_destroy(struct terminal *term);  /* removes the file */

void aura_status_cmd_started(struct terminal *term);
void aura_status_cmd_finished(struct terminal *term, uint32_t duration_ms);
void aura_status_cwd_changed(struct terminal *term);
void aura_status_bell(struct terminal *term);
void aura_status_notification(struct terminal *term, const char *title, const char *body);
void aura_status_key(struct terminal *term);
void aura_status_classify(struct terminal *term); /* after pty reads, while pending */
