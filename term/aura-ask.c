#include "aura-ask.h"

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#define LOG_MODULE "aura-ask"
#define LOG_ENABLE_DBG 0
#include "log.h"
#include "selection.h"
#include "spawn.h"
#include "terminal.h"
#include "xmalloc.h"

#define ASK_HELPER "aura-term-ask"

/* 'single-quoted' for sh, as swaymsg exec runs the line through sh -c */
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

static char *
write_request(const struct terminal *term, const char *selection)
{
    const char *xdg = getenv("XDG_RUNTIME_DIR");
    if (xdg == NULL || xdg[0] == '\0') {
        LOG_ERR("Ask Aura: XDG_RUNTIME_DIR is not set");
        return NULL;
    }

    char *dir = xasprintf("%s/aura-os", xdg);
    mkdir(dir, 0700);
    free(dir);
    dir = xasprintf("%s/aura-os/ask", xdg);
    mkdir(dir, 0700);

    static unsigned counter;
    char *path = NULL;
    int fd = -1;
    for (int tries = 0; tries < 100 && fd < 0; tries++) {
        free(path);
        path = xasprintf("%s/%d-%u.json", dir, term->slave, ++counter);
        fd = open(path, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0600);
        if (fd < 0 && errno != EEXIST)
            break;
    }
    free(dir);

    FILE *out = fd >= 0 ? fdopen(fd, "w") : NULL;
    if (out == NULL) {
        LOG_ERRNO("Ask Aura: %s: failed to create request", path);
        if (fd >= 0)
            close(fd);
        free(path);
        return NULL;
    }

    const struct aura_failed_block *f = &term->aura.failed;
    if (selection != NULL) {
        fputs("{\"selection\":", out);
        aura_json_str(out, selection);
        fputs(",\"cwd\":", out);
        aura_json_str(out, term->cwd);
    } else {
        fputs("{\"command\":", out);
        aura_json_str(out, f->command);
        fputs(",\"output\":", out);
        aura_json_str(out, f->output);
        fprintf(out, ",\"exit\":%d,\"cwd\":", f->exit_code);
        aura_json_str(out, f->cwd != NULL ? f->cwd : term->cwd);
    }
    fputs("}\n", out);

    if (fclose(out) != 0) {
        LOG_ERRNO("Ask Aura: %s: failed to write request", path);
        unlink(path);
        free(path);
        return NULL;
    }
    return path;
}

bool
aura_ask(struct terminal *term)
{
    char *selection = selection_to_text(term);
    if (selection != NULL && selection[0] == '\0') {
        free(selection);
        selection = NULL;
    }

    if (selection == NULL && term->aura.failed.command == NULL) {
        LOG_INFO("Ask Aura: nothing selected and no failed command yet");
        return false;
    }

    char *request = write_request(term, selection);
    free(selection);
    if (request == NULL)
        return false;

    /* The answer window: same terminal (foot_exe is aura-term in server
     * mode, aura-termd standalone), app-id aura-ask for sway rules */
    const char *cwd = term->cwd != NULL ? term->cwd : "/";
    pid_t pid;

    if (getenv("SWAYSOCK") != NULL) {
        char *line = NULL;
        size_t len = 0;
        FILE *out = open_memstream(&line, &len);
        sh_quote(out, term->foot_exe);
        fputs(" --app-id=aura-ask --title='Ask Aura' --working-directory=", out);
        sh_quote(out, cwd);
        fputs(" " ASK_HELPER " --input ", out);
        sh_quote(out, request);
        fputs(" --delete-input", out);
        fclose(out);

        char *argv[] = {"swaymsg", "exec", line, NULL};
        pid = spawn(term->reaper, cwd, argv, -1, -1, -1, NULL, NULL, NULL);
        free(line);
    } else {
        char *argv[] = {
            term->foot_exe, "--app-id=aura-ask", "--title=Ask Aura",
            ASK_HELPER, "--input", request, "--delete-input", NULL,
        };
        pid = spawn(term->reaper, cwd, argv, -1, -1, -1, NULL, NULL, NULL);
    }

    if (pid < 0) {
        LOG_ERR("Ask Aura: failed to open the answer window");
        unlink(request);
    }
    free(request);
    return pid >= 0;
}
