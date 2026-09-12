/* Drawing half of aura command blocks: only linked into aura-termd
 * (render.c), not into foot's vtlib/pgolib used by pgo and tests. */
#include "aura-blocks.h"

#include <stdio.h>

#include <fcft/fcft.h>

#include "srgb.h"
#include "terminal.h"
#include "wayland.h"

#define LABEL_MAX 32

static void
format_duration(uint32_t ms, char *buf, size_t size)
{
    if (ms < 1000)
        snprintf(buf, size, "%ums", ms);
    else if (ms < 60 * 1000)
        snprintf(buf, size, "%u.%us", ms / 1000, ms % 1000 / 100);
    else if (ms < 60 * 60 * 1000)
        snprintf(buf, size, "%um%02us", ms / 60000, ms / 1000 % 60);
    else
        snprintf(buf, size, "%uh%02um", ms / 3600000, ms / 60000 % 60);
}

/* "✓ 1.2s" or "✗ 2 · 1.2s"; every character is one cell wide */
static int
build_label(const struct aura_row_mark *m, char32_t *label)
{
    char dur[16];
    format_duration(m->duration_ms, dur, sizeof(dur));

    int n = 0;
    if (m->exit_code == 0) {
        label[n++] = U'✓';
        label[n++] = U' ';
    } else {
        char code[4];
        snprintf(code, sizeof(code), "%u", m->exit_code);
        label[n++] = U'✗';
        label[n++] = U' ';
        for (const char *p = code; *p != '\0'; p++)
            label[n++] = *p;
        label[n++] = U' ';
        label[n++] = U'·';
        label[n++] = U' ';
    }
    for (const char *p = dur; *p != '\0' && n < LABEL_MAX; p++)
        label[n++] = *p;
    return n;
}

/* First column of the label (including a one-cell gap), or -1 if the
 * row's trailing blanks can't hold it: never draw over text */
static int
label_col(const struct terminal *term, const struct row *row, int len)
{
    const int cols = term->cols;
    const int start = cols - len - 1;
    if (start < 0)
        return -1;

    for (int c = start; c < cols; c++) {
        char32_t wc = row->cells[c].wc;
        if (wc != 0 && wc != U' ')
            return -1;
    }
    return start;
}

void
aura_blocks_row_pre_render(const struct terminal *term, struct row *row)
{
    struct aura_row_mark *m = &row->shell_integration.aura;
    if (m->state != AURA_BLOCK_DONE && m->drawn_col < 0)
        return;

    int col = -1;
    if (m->state == AURA_BLOCK_DONE) {
        char32_t label[LABEL_MAX];
        col = label_col(term, row, build_label(m, label));
    }

    /* Repaint both the old and new label area */
    int first = term->cols;
    if (m->drawn_col >= 0 && m->drawn_col < first)
        first = m->drawn_col;
    if (col >= 0 && col < first)
        first = col;

    for (int c = first; c < term->cols; c++)
        row->cells[c].attrs.clean = 0;

    m->drawn_col = col;
}

void
aura_blocks_row_post_render(const struct terminal *term, pixman_image_t *pix,
                            const struct row *row, int row_no)
{
    const struct aura_row_mark *m = &row->shell_integration.aura;
    if (m->drawn_col < 0 || m->state != AURA_BLOCK_DONE)
        return;

    char32_t label[LABEL_MAX];
    const int len = build_label(m, label);

    /* Failure in the palette's red, success in its dim grey */
    const uint32_t c = term->colors.table[m->exit_code == 0 ? 8 : 1];
    const bool srgb = wayl_do_linear_blending(term->wl, term->conf);
    pixman_color_t color = srgb
        ? (pixman_color_t){
              .red = srgb_decode_8_to_16(c >> 16 & 0xff),
              .green = srgb_decode_8_to_16(c >> 8 & 0xff),
              .blue = srgb_decode_8_to_16(c & 0xff),
              .alpha = 0xffff}
        : (pixman_color_t){
              .red = (c >> 16 & 0xff) * 0x101,
              .green = (c >> 8 & 0xff) * 0x101,
              .blue = (c & 0xff) * 0x101,
              .alpha = 0xffff};
    pixman_image_t *src = pixman_image_create_solid_fill(&color);

    const int y = term->margins.top + row_no * term->cell_height;
    int x = term->margins.left + (m->drawn_col + 1) * term->cell_width;

    for (int i = 0; i < len; i++, x += term->cell_width) {
        if (label[i] == U' ')
            continue;

        const struct fcft_glyph *glyph = fcft_rasterize_char_utf32(
            term->fonts[0], label[i], term->font_subpixel);
        if (glyph == NULL)
            continue;

        const int gx = x + term->font_x_ofs + glyph->x;
        const int gy = y + term->font_baseline - glyph->y;

        if (glyph->is_color_glyph)
            pixman_image_composite32(PIXMAN_OP_OVER, glyph->pix, NULL, pix,
                                     0, 0, 0, 0, gx, gy, glyph->width, glyph->height);
        else
            pixman_image_composite32(PIXMAN_OP_OVER, src, glyph->pix, pix,
                                     0, 0, 0, 0, gx, gy, glyph->width, glyph->height);
    }

    pixman_image_unref(src);
}
