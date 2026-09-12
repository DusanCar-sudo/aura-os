#include "aura-blocks.h"

#include <stdlib.h>
#include <string.h>

#define LOG_MODULE "aura-blocks"
#define LOG_ENABLE_DBG 0
#include "log.h"
#include "extract.h"
#include "grid.h"
#include "selection.h"
#include "terminal.h"
#include "xmalloc.h"

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

static uint32_t
elapsed_ms(const struct terminal *term)
{
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    int64_t ms =
        (int64_t)(now.tv_sec - term->aura.cmd_started.tv_sec) * 1000 +
        (now.tv_nsec - term->aura.cmd_started.tv_nsec) / 1000000;
    return ms < 0 ? 0 : ms > UINT32_MAX ? UINT32_MAX : (uint32_t)ms;
}

void
aura_blocks_prompt(struct terminal *term)
{
    /* A new prompt without D: the shell doesn't report D. No label for
     * the block, but the pane is no longer running anything. */
    if (term->aura.cmd_running) {
        term->aura.cmd_running = false;
        aura_status_cmd_finished(term, elapsed_ms(term));
    }
}

void
aura_blocks_cmd_line(struct terminal *term)
{
    struct grid *grid = term->grid;
    term->aura.cmd_line_valid = true;
    term->aura.cmd_line_row = grid_row_absolute(grid, grid->cursor.point.row);
    term->aura.cmd_line_col = grid->cursor.point.col;
}

/* Text of grid rows [start_row:start_col, end_row:end_col), absolute rows */
static char *
grid_text(const struct terminal *term, int start_row, int start_col,
          int end_row, int end_col)
{
    struct extraction_context *ctx = extract_begin(SELECTION_NONE, true);
    if (ctx == NULL)
        return NULL;

    const struct grid *grid = term->grid;
    for (int r = start_row, c0 = start_col; ; r = (r + 1) & (grid->num_rows - 1), c0 = 0) {
        const struct row *row = grid->rows[r];
        if (row == NULL)
            break;
        const int c1 = r == end_row ? end_col : term->cols;
        for (int c = c0; c < c1 && c < term->cols; c++) {
            if (!extract_one(term, row, &row->cells[c], c, ctx))
                break;
        }
        if (r == end_row)
            break;
    }

    char *text = NULL;
    size_t len = 0;
    if (!extract_finish(ctx, &text, &len))
        return NULL;

    /* Trim surrounding whitespace/newlines */
    while (len > 0 && strchr(" \t\r\n", text[len - 1]) != NULL)
        text[--len] = '\0';
    size_t lead = strspn(text, " \t\r\n");
    memmove(text, text + lead, len - lead + 1);
    return text;
}

/* The command the user typed: from the B mark to where C was emitted */
static void
capture_command(struct terminal *term)
{
    free(term->aura.cmd_text);
    term->aura.cmd_text = NULL;

    if (!term->aura.cmd_line_valid)
        return;
    term->aura.cmd_line_valid = false;

    const struct grid *grid = term->grid;
    int end_row = grid_row_absolute(grid, grid->cursor.point.row);
    int end_col = grid->cursor.point.col;
    if (end_col == 0) {
        end_row = (end_row - 1 + grid->num_rows) & (grid->num_rows - 1);
        end_col = term->cols;
    }

    /* Rows may have been reflowed or recycled since B; only trust a
     * short span */
    const int span = (end_row - term->aura.cmd_line_row) & (grid->num_rows - 1);
    if (span > 32)
        return;

    term->aura.cmd_text = grid_text(
        term, term->aura.cmd_line_row, term->aura.cmd_line_col, end_row, end_col);
}

static void
failed_block_free(struct aura_failed_block *f)
{
    free(f->command);
    free(f->output);
    free(f->cwd);
    *f = (struct aura_failed_block){0};
}

static void
remember_failed_block(struct terminal *term, int exit_code)
{
    struct aura_failed_block *f = &term->aura.failed;
    failed_block_free(f);

    f->command = xstrdup(term->aura.cmd_text != NULL ? term->aura.cmd_text : "");
    f->cwd = term->cwd != NULL ? xstrdup(term->cwd) : NULL;
    f->exit_code = exit_code;

    char *out = NULL;
    size_t len = 0;
    if (term_command_output_to_text(term, &out, &len)) {
        if (len > AURA_FAILED_OUTPUT_MAX) {
            /* Keep the tail, starting on a UTF-8 character boundary */
            size_t skip = len - AURA_FAILED_OUTPUT_MAX;
            while (skip < len && ((unsigned char)out[skip] & 0xc0) == 0x80)
                skip++;
            memmove(out, out + skip, len - skip + 1);
            len -= skip;
        }
        while (len > 0 && (out[len - 1] == '\n' || out[len - 1] == '\r'))
            out[--len] = '\0';
        f->output = out;
    } else
        f->output = xstrdup("");
}

void
aura_blocks_free(struct terminal *term)
{
    free(term->aura.cmd_text);
    term->aura.cmd_text = NULL;
    failed_block_free(&term->aura.failed);
}

void
aura_blocks_cmd_executed(struct terminal *term)
{
    capture_command(term);

    struct row *row = command_row(term);
    if (row == NULL)
        return;

    row->shell_integration.aura.state = AURA_BLOCK_RUNNING;
    term->aura.cmd_running = true;
    clock_gettime(CLOCK_MONOTONIC, &term->aura.cmd_started);
    aura_status_cmd_started(term);
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

    const uint32_t ms = elapsed_ms(term);
    aura_status_cmd_finished(term, ms);

    if (exit_code != 0)
        remember_failed_block(term, exit_code < 0 || exit_code > 255 ? 255 : (int)exit_code);

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
            m->duration_ms = ms;
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
