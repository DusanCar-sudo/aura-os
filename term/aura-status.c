#include "aura-status.h"

#include <ctype.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#define LOG_MODULE "aura-status"
#define LOG_ENABLE_DBG 0
#include "log.h"
#include "notify.h"
#include "terminal.h"
#include "util.h"
#include "xmalloc.h"

/* A job that ran at least this long ends in "done" rather than "idle" */
#define LONG_JOB_MS 10000

static const char *const state_names[] = {
    [AURA_PANE_IDLE] = "idle",
    [AURA_PANE_WORKING] = "working",
    [AURA_PANE_DONE] = "done",
    [AURA_PANE_INPUT] = "input",
};

static const char *const agent_names[] = {
    [AURA_AGENT_NONE] = NULL,
    [AURA_AGENT_CLAUDE] = "claude",
    [AURA_AGENT_CODEX] = "codex",
    [AURA_AGENT_AURA] = "aura",
};

/* Branch of the git repo containing dir: reads .git/HEAD directly (no git
 * process), follows "gitdir:" files (worktrees, submodules). NULL if none. */
static char *
git_branch(const char *dir)
{
    if (dir == NULL || dir[0] != '/')
        return NULL;

    char path[PATH_MAX];
    char cur[PATH_MAX];
    if (strlen(dir) >= sizeof(cur))
        return NULL;
    strcpy(cur, dir);

    for (;;) {
        char head[PATH_MAX] = "";
        struct stat st;

        if (snprintf(path, sizeof(path), "%s/.git",
                     strcmp(cur, "/") == 0 ? "" : cur) >= (int)sizeof(path))
            return NULL;
        if (stat(path, &st) == 0) {
            if (S_ISDIR(st.st_mode)) {
                if (snprintf(head, sizeof(head), "%s/HEAD", path) >= (int)sizeof(head))
                    return NULL;
            }
            else {
                FILE *f = fopen(path, "re");
                char line[PATH_MAX];
                if (f != NULL && fgets(line, sizeof(line), f) != NULL &&
                    strncmp(line, "gitdir: ", 8) == 0)
                {
                    line[strcspn(line, "\n")] = '\0';
                    const char *gd = line + 8;
                    int n = gd[0] == '/'
                        ? snprintf(head, sizeof(head), "%s/HEAD", gd)
                        : snprintf(head, sizeof(head), "%s/%s/HEAD", cur, gd);
                    if (n >= (int)sizeof(head))
                        head[0] = '\0';
                }
                if (f != NULL)
                    fclose(f);
            }
        }

        if (head[0] != '\0') {
            FILE *f = fopen(head, "re");
            if (f == NULL)
                return NULL;
            char line[256];
            char *ret = NULL;
            if (fgets(line, sizeof(line), f) != NULL) {
                line[strcspn(line, "\n")] = '\0';
                if (strncmp(line, "ref: refs/heads/", 16) == 0)
                    ret = xstrdup(line + 16);
                else if (strncmp(line, "ref: ", 5) == 0)
                    ret = xstrdup(line + 5);
                else if (strlen(line) >= 7)
                    ret = xstrndup(line, 7);  /* detached HEAD: short sha */
            }
            fclose(f);
            return ret;
        }

        char *slash = strrchr(cur, '/');
        if (slash == NULL || strcmp(cur, "/") == 0)
            return NULL;
        if (slash == cur)
            cur[1] = '\0';
        else
            *slash = '\0';
    }
}

static void
json_str(FILE *out, const char *s)
{
    if (s == NULL) {
        fputs("null", out);
        return;
    }
    fputc('"', out);
    for (const unsigned char *p = (const unsigned char *)s; *p != '\0'; p++) {
        if (*p == '"' || *p == '\\')
            fprintf(out, "\\%c", *p);
        else if (*p < 0x20)
            fprintf(out, "\\u%04x", *p);
        else
            fputc(*p, out);
    }
    fputc('"', out);
}

static void
write_status(struct terminal *term)
{
    struct aura_status *st = &term->aura.status;
    if (st->path == NULL)
        return;

    /* Before the shell reports OSC 7, fall back to its /proc cwd */
    const char *cwd = term->cwd;
    char proc_cwd[PATH_MAX];
    if (cwd == NULL) {
        char link[64];
        snprintf(link, sizeof(link), "/proc/%d/cwd", term->slave);
        ssize_t n = readlink(link, proc_cwd, sizeof(proc_cwd) - 1);
        if (n > 0) {
            proc_cwd[n] = '\0';
            cwd = proc_cwd;
        }
    }

    char *json = NULL;
    size_t len = 0;
    FILE *out = open_memstream(&json, &len);
    if (out == NULL)
        return;

    fprintf(out, "{\"pid\":%d,\"cwd\":", term->slave);
    json_str(out, cwd);
    fputs(",\"branch\":", out);
    json_str(out, st->branch);
    fputs(",\"agent\":", out);
    json_str(out, agent_names[st->agent]);
    fputs(",\"state\":", out);
    json_str(out, state_names[st->state]);
    fprintf(out, ",\"since\":%lld}\n", (long long)st->since);
    fclose(out);

    if (st->last_json != NULL && streq(st->last_json, json)) {
        free(json);
        return;
    }

    /* Atomic: readers see the old or the new file, never a partial one */
    char *tmp = xasprintf("%s.tmp", st->path);
    int fd = open(tmp, O_WRONLY | O_CREAT | O_TRUNC | O_CLOEXEC, 0600);
    bool ok = fd >= 0 && write(fd, json, len) == (ssize_t)len;
    if (fd >= 0)
        close(fd);
    ok = ok && rename(tmp, st->path) == 0;
    if (!ok) {
        LOG_ERRNO("%s: failed to write pane status", st->path);
        unlink(tmp);
        free(json);
    } else {
        free(st->last_json);
        st->last_json = json;
    }
    free(tmp);
}

static void
set_state(struct terminal *term, enum aura_pane_state state)
{
    struct aura_status *st = &term->aura.status;
    if (st->state == state)
        return;
    st->state = state;
    st->since = time(NULL);
    write_status(term);
}

static void
notify_needs_input(struct terminal *term)
{
    const char *agent = agent_names[term->aura.status.agent];
    notify_notify(term, &(struct notification){
        .title = xstrdup(term->window_title != NULL ? term->window_title : "aura-term"),
        .body = xasprintf("%s needs input", agent != NULL ? agent : "job"),
        .expire_time = -1,
        .focus = true,
    });
}

void
aura_status_init(struct terminal *term)
{
    struct aura_status *st = &term->aura.status;
    *st = (struct aura_status){.state = AURA_PANE_IDLE, .since = time(NULL)};

    const char *xdg = getenv("XDG_RUNTIME_DIR");
    if (xdg == NULL || xdg[0] == '\0' || term->slave <= 0)
        return;

    char *dir = xasprintf("%s/aura-os", xdg);
    if (mkdir(dir, 0700) < 0 && errno != EEXIST)
        LOG_ERRNO("%s: failed to create", dir);
    free(dir);
    dir = xasprintf("%s/aura-os/panes", xdg);
    if (mkdir(dir, 0700) < 0 && errno != EEXIST)
        LOG_ERRNO("%s: failed to create", dir);
    free(dir);

    st->path = xasprintf("%s/aura-os/panes/%d.json", xdg, term->slave);
    st->branch = git_branch(term->cwd);
    write_status(term);
}

void
aura_status_destroy(struct terminal *term)
{
    struct aura_status *st = &term->aura.status;
    if (st->path != NULL)
        unlink(st->path);
    free(st->path);
    free(st->branch);
    free(st->last_json);
    *st = (struct aura_status){0};
}

void
aura_status_cmd_started(struct terminal *term)
{
    struct aura_status *st = &term->aura.status;
    st->agent = AURA_AGENT_NONE;
    st->classify_pending = true;  /* the shell hasn't exec:d the command yet */
    set_state(term, AURA_PANE_WORKING);
}

void
aura_status_cmd_finished(struct terminal *term, uint32_t duration_ms)
{
    struct aura_status *st = &term->aura.status;
    const bool was_agent = st->agent != AURA_AGENT_NONE;
    const bool attention = st->state == AURA_PANE_DONE || st->state == AURA_PANE_INPUT;

    st->classify_pending = false;
    st->agent = AURA_AGENT_NONE;

    if (was_agent || attention || duration_ms >= LONG_JOB_MS)
        set_state(term, AURA_PANE_DONE);
    else
        set_state(term, AURA_PANE_IDLE);

    write_status(term);  /* agent -> null, even if the state is unchanged */
}

void
aura_status_cwd_changed(struct terminal *term)
{
    struct aura_status *st = &term->aura.status;
    free(st->branch);
    st->branch = git_branch(term->cwd);
    write_status(term);
}

static enum aura_agent
agent_from_name(const char *s)
{
    if (s == NULL || s[0] == '\0')
        return AURA_AGENT_NONE;

    const char *base = strrchr(s, '/');
    base = base != NULL ? base + 1 : s;

    if (streq(base, "claude") || strstr(s, "claude-code") != NULL)
        return AURA_AGENT_CLAUDE;
    if (streq(base, "codex") || streq(base, "codex.js") || strstr(s, "@openai/codex") != NULL)
        return AURA_AGENT_CODEX;
    if (streq(base, "aura"))
        return AURA_AGENT_AURA;
    return AURA_AGENT_NONE;
}

/* Identify the pty's foreground program. Event-driven: runs after pty
 * reads only while a command's program is still unknown. */
void
aura_status_classify(struct terminal *term)
{
    struct aura_status *st = &term->aura.status;
    if (!st->classify_pending)
        return;

    pid_t fg = tcgetpgrp(term->ptmx);
    if (fg <= 0 || fg == term->slave)
        return;  /* still the shell: try again on the next read */

    st->classify_pending = false;

    char path[64];
    char buf[512];
    enum aura_agent agent = AURA_AGENT_NONE;

    snprintf(path, sizeof(path), "/proc/%d/comm", fg);
    FILE *f = fopen(path, "re");
    if (f != NULL) {
        if (fgets(buf, sizeof(buf), f) != NULL) {
            buf[strcspn(buf, "\n")] = '\0';
            agent = agent_from_name(buf);
        }
        fclose(f);
    }

    /* Script-wrapped agents: "node .../claude", argv[0] and argv[1] */
    if (agent == AURA_AGENT_NONE) {
        snprintf(path, sizeof(path), "/proc/%d/cmdline", fg);
        int fd = open(path, O_RDONLY | O_CLOEXEC);
        if (fd >= 0) {
            ssize_t n = read(fd, buf, sizeof(buf) - 1);
            close(fd);
            if (n > 0) {
                buf[n] = '\0';
                agent = agent_from_name(buf);
                size_t first = strlen(buf);
                if (agent == AURA_AGENT_NONE && (ssize_t)first + 1 < n)
                    agent = agent_from_name(buf + first + 1);
            }
        }
    }

    if (agent != st->agent) {
        st->agent = agent;
        LOG_DBG("pid %d: agent=%s", fg, agent_names[agent]);
        write_status(term);
    }
}

void
aura_status_bell(struct terminal *term)
{
    if (!term->aura.cmd_running)
        return;  /* bell at the prompt, e.g. tab completion */

    struct aura_status *st = &term->aura.status;
    st->classify_pending = st->classify_pending || st->agent == AURA_AGENT_NONE;
    aura_status_classify(term);

    if (st->agent != AURA_AGENT_NONE) {
        const bool was_input = st->state == AURA_PANE_INPUT;
        set_state(term, AURA_PANE_INPUT);
        if (!was_input)
            notify_needs_input(term);
    } else
        set_state(term, AURA_PANE_DONE);
}

static bool
looks_like_question(const char *s)
{
    if (s == NULL)
        return false;

    static const char *const words[] = {
        "permission", "approv", "allow", "confirm", "waiting", "input", "?",
    };

    char lower[512];
    size_t i = 0;
    for (; s[i] != '\0' && i < sizeof(lower) - 1; i++)
        lower[i] = tolower((unsigned char)s[i]);
    lower[i] = '\0';

    for (size_t w = 0; w < sizeof(words) / sizeof(words[0]); w++) {
        if (strstr(lower, words[w]) != NULL)
            return true;
    }
    return false;
}

/* OSC 9 / 777 / 99. foot shows these notifications itself, so no extra
 * mako notification here. */
void
aura_status_notification(struct terminal *term, const char *title, const char *body)
{
    if (!term->aura.cmd_running)
        return;

    struct aura_status *st = &term->aura.status;
    st->classify_pending = st->classify_pending || st->agent == AURA_AGENT_NONE;
    aura_status_classify(term);

    if (st->agent != AURA_AGENT_NONE || looks_like_question(title) || looks_like_question(body))
        set_state(term, AURA_PANE_INPUT);
    else
        set_state(term, AURA_PANE_DONE);
}

void
aura_status_key(struct terminal *term)
{
    struct aura_status *st = &term->aura.status;
    if (st->state != AURA_PANE_DONE && st->state != AURA_PANE_INPUT)
        return;
    set_state(term, term->aura.cmd_running ? AURA_PANE_WORKING : AURA_PANE_IDLE);
}
