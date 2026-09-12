#include "aura-split.h"

#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define LOG_MODULE "aura-split"
#define LOG_ENABLE_DBG 0
#include "log.h"
#include "spawn.h"
#include "terminal.h"
#include "wayland.h"
#include "xmalloc.h"

#define PLACE_TAG_PREFIX "aura-split-"

static void
sh_quote(FILE *out, const char *s)
{
    fputc('\'', out);
    for (; *s != '\0'; s++) {
        if (*s == '\'')
            fputs("'\\''", out);
        else
            fputc(*s, out);
    }
    fputc('\'', out);
}

bool
aura_split(struct terminal *term, enum aura_split_dir dir)
{
    /* This pane's cwd: OSC 7 if the shell reports it, else the shell's */
    char proc_cwd[PATH_MAX];
    const char *cwd = term->cwd;
    if (cwd == NULL) {
        char link[64];
        snprintf(link, sizeof(link), "/proc/%d/cwd", term->slave);
        ssize_t n = readlink(link, proc_cwd, sizeof(proc_cwd) - 1);
        if (n > 0) {
            proc_cwd[n] = '\0';
            cwd = proc_cwd;
        }
    }
    if (cwd == NULL)
        cwd = getenv("HOME") != NULL ? getenv("HOME") : "/";

    if (getenv("SWAYSOCK") == NULL) {
        char *wd = xasprintf("--working-directory=%s", cwd);
        char *argv[] = {term->foot_exe, wd, NULL};
        pid_t pid = spawn(term->reaper, cwd, argv, -1, -1, -1, NULL, NULL, NULL);
        free(wd);
        return pid >= 0;
    }

    const bool horizontal = dir == AURA_SPLIT_LEFT || dir == AURA_SPLIT_RIGHT;
    const char *place = dir == AURA_SPLIT_LEFT ? "left" : dir == AURA_SPLIT_UP ? "up" : NULL;

    /* The key went to this pane, so it is sway's focused window: split it
     * and open the new pane after it; left/up panes move themselves */
    char *line = NULL;
    size_t len = 0;
    FILE *out = open_memstream(&line, &len);
    fprintf(out, "split %s; exec ", horizontal ? "h" : "v");
    sh_quote(out, term->foot_exe);
    fputs(" --working-directory=", out);
    sh_quote(out, cwd);
    if (place != NULL)
        fprintf(out, " --override=toplevel-tag=" PLACE_TAG_PREFIX "%s", place);
    fclose(out);

    char *argv[] = {"swaymsg", "-q", line, NULL};
    pid_t pid = spawn(term->reaper, cwd, argv, -1, -1, -1, NULL, NULL, NULL);
    free(line);
    if (pid < 0)
        LOG_ERR("split: failed to run swaymsg");
    return pid >= 0;
}

bool
aura_split_parse_tag(struct terminal *term, const char *tag)
{
    if (tag == NULL || strncmp(tag, PLACE_TAG_PREFIX, strlen(PLACE_TAG_PREFIX)) != 0)
        return false;

    const char *place = tag + strlen(PLACE_TAG_PREFIX);
    term->aura.split_place =
        strcmp(place, "left") == 0 ? AURA_SPLIT_LEFT :
        strcmp(place, "up") == 0 ? AURA_SPLIT_UP : AURA_SPLIT_NONE;
    return true;
}

void
aura_split_first_configure(struct wl_window *win)
{
    struct terminal *term = win->term;
    const enum aura_split_dir place = term->aura.split_place;
    if (place == AURA_SPLIT_NONE)
        return;
    term->aura.split_place = AURA_SPLIT_NONE;

    /* sway maps the window on its first commit, right after this
     * configure; retry briefly until the tag criteria match */
    char *cmd = xasprintf(
        "for i in 1 2 3 4 5 6 7 8 9 10; do "
        "swaymsg -q '[tag=\"aura-pane-%d\"] move %s' && exit 0; sleep 0.05; done",
        term->slave, place == AURA_SPLIT_LEFT ? "left" : "up");
    char *argv[] = {"/bin/sh", "-c", cmd, NULL};
    spawn(term->reaper, NULL, argv, -1, -1, -1, NULL, NULL, NULL);
    free(cmd);
}
