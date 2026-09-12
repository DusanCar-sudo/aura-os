#pragma once

/*
 * Aura command blocks: OSC 133 exit code + duration, shown right-aligned
 * on the row the command was typed on, and copying the last block's
 * output. Foot already tracks the A/C/D marks for prompt jumping and
 * pipe-command-output; this adds the per-block status on top.
 *
 * Kept free of terminal.h so terminal.h can embed these structs.
 */

#include <stdbool.h>
#include <stdint.h>
#include <time.h>

#include <pixman.h>

#include "aura-status.h"

struct terminal;
struct row;
struct seat;

enum aura_block_state {
    AURA_BLOCK_NONE = 0,
    AURA_BLOCK_RUNNING,   /* OSC 133;C seen, waiting for D */
    AURA_BLOCK_DONE,      /* OSC 133;D seen: exit code + duration valid */
};

/* Lives in row->shell_integration; 8 bytes per row */
struct aura_row_mark {
    uint32_t duration_ms;
    int16_t drawn_col;    /* first column of the label last drawn, -1 = none */
    uint8_t exit_code;
    uint8_t state;        /* enum aura_block_state */
};

/* Lives in struct terminal */
struct aura_term {
    bool cmd_running;
    struct timespec cmd_started;
    struct aura_status status;
};

static inline void
aura_row_mark_reset(struct aura_row_mark *m)
{
    *m = (struct aura_row_mark){.drawn_col = -1};
}

/* OSC 133 hooks, called after foot's own handling of each mark */
void aura_blocks_prompt(struct terminal *term);            /* A */
void aura_blocks_cmd_executed(struct terminal *term);      /* C */
void aura_blocks_cmd_finished(struct terminal *term, const char *params); /* D */

/* render_row() hooks. pre: marks the label's cells for repaint;
 * post: draws the label over them. Safe on render worker threads. */
void aura_blocks_row_pre_render(const struct terminal *term, struct row *row);
void aura_blocks_row_post_render(const struct terminal *term, pixman_image_t *pix,
                                 const struct row *row, int row_no);

/* Ctrl+Shift+Y: last finished block's output to the clipboard */
bool aura_blocks_copy_last(struct seat *seat, struct terminal *term, uint32_t serial);
