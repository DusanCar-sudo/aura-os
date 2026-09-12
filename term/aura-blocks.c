#include "aura-blocks.h"

#include <stdlib.h>
#include <string.h>

#define LOG_MODULE "aura-blocks"
#define LOG_ENABLE_DBG 0
#include "log.h"
#include "grid.h"
#include "selection.h"
#include "terminal.h"

/*
 * Row the user typed the command on. At OSC 133;C the shell has
 * already moved to the first output line, so that is normally the
 * row above the cursor.
 */
static struct row *
command_row(struct terminal *term)
{
    struct grid *grid = term->grid;
    int abs = grid_row_absolute(grid, grid->cursor.point.row);
    if (grid->cursor.point.col == 0)
        abs = (abs - 1 + grid->num_rows) & (grid->num_rows - 1);
    return grid->rows[abs];
}

void
aura_blocks_prompt(struct terminal *term)
{
    /* A new prompt without D: the shell doesn't report D, forget the block */
    term->aura.cmd_running = false;
}

void
aura_blocks_cmd_executed(struct terminal *term)
{
    struct row *row = command_row(term);
    if (row == NULL)
        return;

    row->shell_integration.aura.state = AURA_BLOCK_RUNNING;
    term->aura.cmd_running = true;
    clock_gettime(CLOCK_MONOTONIC, &term->aura.cmd_started);
}

void
aura_blocks_cmd_finished(struct terminal *term, const char *params)
{
    if (!term->aura.cmd_running)
        return;  /* D without C: empty command line */
    term->aura.cmd_running = false;

    /* OSC 133;D[;exit[;k=v...]] */
    long exit_code = 0;
    if (params[0] == ';')
        exit_code = strtol(params + 1, NULL, 10);

    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    int64_t ms =
        (int64_t)(now.tv_sec - term->aura.cmd_started.tv_sec) * 1000 +
        (now.tv_nsec - term->aura.cmd_started.tv_nsec) / 1000000;

    /* Walk back to the row marked at C; it may have scrolled off, in
     * which case the block simply gets no label */
    struct grid *grid = term->grid;
    int abs = grid_row_absolute(grid, grid->cursor.point.row);
    for (int i = 0; i < grid->num_rows; i++) {
        struct row *row = grid->rows[abs];
        if (row == NULL)
            break;

        struct aura_row_mark *m = &row->shell_integration.aura;
        if (m->state == AURA_BLOCK_RUNNING) {
            m->state = AURA_BLOCK_DONE;
            m->exit_code = exit_code < 0 || exit_code > 255 ? 255 : (uint8_t)exit_code;
            m->duration_ms = ms < 0 ? 0 : ms > UINT32_MAX ? UINT32_MAX : (uint32_t)ms;
            row->dirty = true;
            LOG_DBG("block done: exit=%u, %ums", m->exit_code, m->duration_ms);
            return;
        }
        abs = (abs - 1 + grid->num_rows) & (grid->num_rows - 1);
    }
}

bool
aura_blocks_copy_last(struct seat *seat, struct terminal *term, uint32_t serial)
{
    char *text = NULL;
    size_t len = 0;

    if (!term_command_output_to_text(term, &text, &len))
        return false;

    if (!text_to_clipboard(seat, term, text, serial)) {
        free(text);
        return false;
    }
    return true;
}
