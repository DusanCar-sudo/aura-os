/*
 * aura-term-ask: the Ask Aura bridge of aura-term.
 *
 *   aura-term-ask [--input FILE [--delete-input]] [--no-prompt]
 *
 * Reads one JSON object (stdin, or FILE):
 *   {"selection": "...", "cwd": "..."}                         or
 *   {"command": "...", "output": "...", "exit": 2, "cwd": "..."}
 * asks Aura over her sidecar (`aura sidecar`, NDJSON over stdio, see
 * aura-code docs/PROTOCOL.md) and streams her answer to stdout.
 *
 * Read-only by construction: the session is created with
 * permission "read-only" and allowedTools [] (no tools), and every
 * approval.request is answered "deny". A command Aura suggests is shown
 * and only run, in this window, after the user presses Enter.
 *
 * If `aura` is not installed, prints what would have been asked.
 */

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

#define TURN_TIMEOUT_S 180
#define BOLD "\033[1m"
#define DIM  "\033[2m"
#define RED  "\033[31m"
#define OFF  "\033[0m"

/* ---- minimal JSON ------------------------------------------------------ */

enum jtype { J_NULL, J_BOOL, J_NUM, J_STR, J_ARR, J_OBJ };

struct jv {
    enum jtype type;
    bool b;
    double num;
    char *str;           /* J_STR */
    size_t n;            /* J_ARR / J_OBJ */
    char **keys;         /* J_OBJ */
    struct jv **items;
};

static void
jv_free(struct jv *v)
{
    if (v == NULL)
        return;
    for (size_t i = 0; i < v->n; i++) {
        if (v->keys != NULL)
            free(v->keys[i]);
        jv_free(v->items[i]);
    }
    free(v->keys);
    free(v->items);
    free(v->str);
    free(v);
}

static void
skip_ws(const char **p)
{
    while (**p == ' ' || **p == '\t' || **p == '\n' || **p == '\r')
        (*p)++;
}

static void
put_utf8(char **out, unsigned cp)
{
    char *o = *out;
    if (cp < 0x80)
        *o++ = cp;
    else if (cp < 0x800) {
        *o++ = 0xc0 | cp >> 6;
        *o++ = 0x80 | (cp & 0x3f);
    } else if (cp < 0x10000) {
        *o++ = 0xe0 | cp >> 12;
        *o++ = 0x80 | (cp >> 6 & 0x3f);
        *o++ = 0x80 | (cp & 0x3f);
    } else {
        *o++ = 0xf0 | cp >> 18;
        *o++ = 0x80 | (cp >> 12 & 0x3f);
        *o++ = 0x80 | (cp >> 6 & 0x3f);
        *o++ = 0x80 | (cp & 0x3f);
    }
    *out = o;
}

static bool
hex4(const char *p, unsigned *v)
{
    *v = 0;
    for (int i = 0; i < 4; i++) {
        char c = p[i];
        *v <<= 4;
        if (c >= '0' && c <= '9') *v |= c - '0';
        else if (c >= 'a' && c <= 'f') *v |= c - 'a' + 10;
        else if (c >= 'A' && c <= 'F') *v |= c - 'A' + 10;
        else return false;
    }
    return true;
}

static char *
parse_string(const char **p)
{
    if (**p != '"')
        return NULL;
    (*p)++;
    /* Output is never longer than input (escapes shrink or stay equal) */
    char *buf = malloc(strlen(*p) + 1), *o = buf;
    if (buf == NULL)
        return NULL;
    while (**p != '"') {
        char c = **p;
        if (c == '\0')
            goto err;
        (*p)++;
        if (c != '\\') {
            *o++ = c;
            continue;
        }
        c = **p;
        (*p)++;
        switch (c) {
        case '"': *o++ = '"'; break;
        case '\\': *o++ = '\\'; break;
        case '/': *o++ = '/'; break;
        case 'b': *o++ = '\b'; break;
        case 'f': *o++ = '\f'; break;
        case 'n': *o++ = '\n'; break;
        case 'r': *o++ = '\r'; break;
        case 't': *o++ = '\t'; break;
        case 'u': {
            unsigned cp, lo;
            if (!hex4(*p, &cp))
                goto err;
            *p += 4;
            if (cp >= 0xd800 && cp < 0xdc00 && (*p)[0] == '\\' && (*p)[1] == 'u' &&
                hex4(*p + 2, &lo) && lo >= 0xdc00 && lo < 0xe000)
            {
                cp = 0x10000 + ((cp - 0xd800) << 10) + (lo - 0xdc00);
                *p += 6;
            }
            put_utf8(&o, cp);
            break;
        }
        default:
            goto err;
        }
    }
    (*p)++;
    *o = '\0';
    return buf;
err:
    free(buf);
    return NULL;
}

static struct jv *parse_value(const char **p, int depth);

static struct jv *
parse_container(const char **p, int depth, bool obj)
{
    struct jv *v = calloc(1, sizeof(*v));
    v->type = obj ? J_OBJ : J_ARR;
    (*p)++;
    skip_ws(p);
    if (**p == (obj ? '}' : ']')) {
        (*p)++;
        return v;
    }
    for (;;) {
        char *key = NULL;
        skip_ws(p);
        if (obj) {
            if ((key = parse_string(p)) == NULL)
                goto err;
            skip_ws(p);
            if (**p != ':') {
                free(key);
                goto err;
            }
            (*p)++;
        }
        struct jv *item = parse_value(p, depth + 1);
        if (item == NULL) {
            free(key);
            goto err;
        }
        v->items = realloc(v->items, (v->n + 1) * sizeof(v->items[0]));
        if (obj)
            v->keys = realloc(v->keys, (v->n + 1) * sizeof(v->keys[0]));
        v->items[v->n] = item;
        if (obj)
            v->keys[v->n] = key;
        v->n++;
        skip_ws(p);
        if (**p == ',') {
            (*p)++;
            continue;
        }
        if (**p == (obj ? '}' : ']')) {
            (*p)++;
            return v;
        }
        goto err;
    }
err:
    jv_free(v);
    return NULL;
}

static struct jv *
parse_value(const char **p, int depth)
{
    if (depth > 64)
        return NULL;
    skip_ws(p);
    switch (**p) {
    case '{': return parse_container(p, depth, true);
    case '[': return parse_container(p, depth, false);
    case '"': {
        char *s = parse_string(p);
        if (s == NULL)
            return NULL;
        struct jv *v = calloc(1, sizeof(*v));
        v->type = J_STR;
        v->str = s;
        return v;
    }
    }

    struct jv *v = calloc(1, sizeof(*v));
    if (strncmp(*p, "true", 4) == 0) { v->type = J_BOOL; v->b = true; *p += 4; }
    else if (strncmp(*p, "false", 5) == 0) { v->type = J_BOOL; *p += 5; }
    else if (strncmp(*p, "null", 4) == 0) { v->type = J_NULL; *p += 4; }
    else {
        char *end;
        v->num = strtod(*p, &end);
        if (end == *p) {
            free(v);
            return NULL;
        }
        v->type = J_NUM;
        *p = end;
    }
    return v;
}

static struct jv *
json_parse(const char *s)
{
    const char *p = s;
    struct jv *v = parse_value(&p, 0);
    skip_ws(&p);
    if (v != NULL && *p != '\0') {
        jv_free(v);
        return NULL;
    }
    return v;
}

static const struct jv *
jget(const struct jv *obj, const char *key)
{
    if (obj == NULL || obj->type != J_OBJ)
        return NULL;
    for (size_t i = 0; i < obj->n; i++) {
        if (strcmp(obj->keys[i], key) == 0)
            return obj->items[i];
    }
    return NULL;
}

static const char *
jstr(const struct jv *obj, const char *key)
{
    const struct jv *v = jget(obj, key);
    return v != NULL && v->type == J_STR ? v->str : NULL;
}

static void
json_write_str(FILE *out, const char *s)
{
    fputc('"', out);
    for (const unsigned char *c = (const unsigned char *)s; *c != '\0'; c++) {
        switch (*c) {
        case '"': fputs("\\\"", out); break;
        case '\\': fputs("\\\\", out); break;
        case '\n': fputs("\\n", out); break;
        case '\r': fputs("\\r", out); break;
        case '\t': fputs("\\t", out); break;
        default:
            if (*c < 0x20)
                fprintf(out, "\\u%04x", *c);
            else
                fputc(*c, out);
        }
    }
    fputc('"', out);
}

/* ---- helpers ----------------------------------------------------------- */

struct buf {
    char *data;
    size_t len, cap;
};

static void
buf_add(struct buf *b, const char *s, size_t n)
{
    if (b->len + n + 1 > b->cap) {
        b->cap = (b->len + n + 1) * 2;
        b->data = realloc(b->data, b->cap);
    }
    memcpy(b->data + b->len, s, n);
    b->len += n;
    b->data[b->len] = '\0';
}

static char *
read_all(FILE *f)
{
    struct buf b = {0};
    char chunk[4096];
    size_t n;
    while ((n = fread(chunk, 1, sizeof(chunk), f)) > 0)
        buf_add(&b, chunk, n);
    if (b.data == NULL)
        buf_add(&b, "", 0);
    return b.data;
}

static bool
in_path(const char *prog)
{
    const char *path = getenv("PATH");
    if (path == NULL)
        return false;
    char *copy = strdup(path), *save = NULL;
    bool found = false;
    for (char *dir = strtok_r(copy, ":", &save); dir != NULL && !found;
         dir = strtok_r(NULL, ":", &save))
    {
        char full[4096];
        snprintf(full, sizeof(full), "%s/%s", dir[0] != '\0' ? dir : ".", prog);
        found = access(full, X_OK) == 0;
    }
    free(copy);
    return found;
}

/* Last ```sh (or bash/shell/zsh/console/unlabelled) block of exactly one
 * command line; NULL if none. A leading "$ " prompt is stripped. */
static char *
suggested_command(const char *answer)
{
    char *found = NULL;
    const char *p = answer;
    while ((p = strstr(p, "```")) != NULL) {
        const char *lang = p + 3;
        const char *nl = strchr(lang, '\n');
        if (nl == NULL)
            break;
        const char *end = strstr(nl + 1, "```");
        if (end == NULL)
            break;

        static const char *const shells[] = {"", "sh", "bash", "zsh", "shell", "console"};
        const size_t lang_len = nl - lang;
        bool shell_lang = false;
        for (size_t i = 0; i < sizeof(shells) / sizeof(shells[0]); i++) {
            if (strlen(shells[i]) == lang_len && strncmp(lang, shells[i], lang_len) == 0)
                shell_lang = true;
        }

        /* Body, trimmed */
        const char *s = nl + 1, *e = end;
        while (s < e && (*s == ' ' || *s == '\n' || *s == '\t'))
            s++;
        while (e > s && (e[-1] == ' ' || e[-1] == '\n' || e[-1] == '\t'))
            e--;
        if (shell_lang && e > s && memchr(s, '\n', e - s) == NULL) {
            if (e - s > 2 && s[0] == '$' && s[1] == ' ')
                s += 2;
            free(found);
            found = strndup(s, e - s);
        }
        p = end + 3;
    }
    return found;
}

static char *
build_prompt(const struct jv *in)
{
    const char *cwd = jstr(in, "cwd");
    const char *sel = jstr(in, "selection");
    const char *cmd = jstr(in, "command");
    const char *out = jstr(in, "output");
    const struct jv *exit_v = jget(in, "exit");

    char *prompt = NULL;
    size_t len = 0;
    FILE *f = open_memstream(&prompt, &len);

    fputs("You are answering in the Ask Aura panel of aura-term, Aura OS's "
          "terminal. This session is read-only: you have no tools and nothing "
          "you say is run automatically. Answer from what is given, briefly "
          "(a few sentences).\n\n", f);

    if (sel != NULL) {
        fprintf(f, "The user selected this terminal text (cwd: %s):\n```\n%s\n```\n"
                   "Explain it, and if something is wrong, how to fix it.\n",
                cwd != NULL ? cwd : "unknown", sel);
    } else {
        fprintf(f, "This command failed in the user's terminal.\n\n"
                   "cwd: %s\n$ %s\nexit code: %d\noutput:\n```\n%s\n```\n"
                   "Explain why it failed.\n",
                cwd != NULL ? cwd : "unknown",
                cmd != NULL ? cmd : "(unknown)",
                exit_v != NULL && exit_v->type == J_NUM ? (int)exit_v->num : -1,
                out != NULL ? out : "");
    }
    fputs("\nIf one shell command would fix or diagnose this, give exactly one, "
          "alone in a ```sh code block.", f);
    fclose(f);
    return prompt;
}

/* ---- sidecar ----------------------------------------------------------- */

struct sidecar {
    pid_t pid;
    FILE *in;       /* to the engine */
    int out_fd;     /* from the engine */
    struct buf line;
    size_t line_start;
};

static bool
sidecar_start(struct sidecar *sc)
{
    int to_child[2], from_child[2];
    if (pipe(to_child) < 0 || pipe(from_child) < 0)
        return false;

    /* Engine diagnostics (stderr) go to a log, never into the answer */
    int log_fd = -1;
    const char *xdg = getenv("XDG_RUNTIME_DIR");
    if (xdg != NULL) {
        char path[4096];
        snprintf(path, sizeof(path), "%s/aura-os/ask/sidecar.log", xdg);
        log_fd = open(path, O_WRONLY | O_CREAT | O_TRUNC | O_CLOEXEC, 0600);
    }
    if (log_fd < 0)
        log_fd = open("/dev/null", O_WRONLY | O_CLOEXEC);

    sc->pid = fork();
    if (sc->pid < 0)
        return false;
    if (sc->pid == 0) {
        dup2(to_child[0], STDIN_FILENO);
        dup2(from_child[1], STDOUT_FILENO);
        dup2(log_fd, STDERR_FILENO);
        close(to_child[1]);
        close(from_child[0]);
        execlp("aura", "aura", "sidecar", (char *)NULL);
        _exit(127);
    }

    close(to_child[0]);
    close(from_child[1]);
    if (log_fd >= 0)
        close(log_fd);
    sc->in = fdopen(to_child[1], "w");
    sc->out_fd = from_child[0];
    return sc->in != NULL;
}

/* Next complete line from the engine, or NULL on EOF/timeout */
static char *
sidecar_line(struct sidecar *sc, time_t deadline)
{
    for (;;) {
        char *start = sc->line.data != NULL ? sc->line.data + sc->line_start : NULL;
        char *nl = start != NULL ? strchr(start, '\n') : NULL;
        if (nl != NULL) {
            *nl = '\0';
            sc->line_start = nl + 1 - sc->line.data;
            return start;
        }

        /* Compact */
        if (sc->line_start > 0) {
            size_t rest = sc->line.len - sc->line_start;
            memmove(sc->line.data, sc->line.data + sc->line_start, rest + 1);
            sc->line.len = rest;
            sc->line_start = 0;
        }

        int left = (int)(deadline - time(NULL));
        if (left <= 0)
            return NULL;
        struct pollfd pfd = {.fd = sc->out_fd, .events = POLLIN};
        int r = poll(&pfd, 1, left * 1000);
        if (r < 0 && errno == EINTR)
            continue;
        if (r <= 0)
            return NULL;

        char chunk[8192];
        ssize_t n = read(sc->out_fd, chunk, sizeof(chunk));
        if (n <= 0)
            return NULL;
        buf_add(&sc->line, chunk, n);
    }
}

static void
send_req(struct sidecar *sc, const char *id, const char *method, const char *params_json)
{
    fprintf(sc->in, "{\"kind\":\"req\",\"id\":\"%s\",\"method\":\"%s\",\"params\":%s}\n",
            id, method, params_json);
    fflush(sc->in);
}

static void
sidecar_stop(struct sidecar *sc)
{
    if (sc->in != NULL)
        fclose(sc->in);
    sc->in = NULL;
    for (int i = 0; i < 30; i++) {
        if (waitpid(sc->pid, NULL, WNOHANG) == sc->pid)
            return;
        usleep(100 * 1000);
    }
    kill(sc->pid, SIGTERM);
    waitpid(sc->pid, NULL, 0);
}

/* Runs one read-only turn; streams the answer to stdout. Returns the full
 * answer text (for the suggested command), NULL on failure. */
static char *
ask_aura(const char *root, const char *prompt)
{
    struct sidecar sc = {0};
    if (!sidecar_start(&sc)) {
        fprintf(stderr, "aura-term-ask: failed to start aura sidecar: %s\n", strerror(errno));
        return NULL;
    }

    /* session.create: read-only, no tools at all */
    char *params = NULL;
    size_t plen = 0;
    FILE *f = open_memstream(&params, &plen);
    fputs("{\"projectRoot\":", f);
    json_write_str(f, root);
    fputs(",\"name\":\"aura-term ask\",\"permission\":\"read-only\",\"allowedTools\":[]}", f);
    fclose(f);
    send_req(&sc, "1", "session.create", params);
    free(params);

    struct buf answer = {0};
    buf_add(&answer, "", 0);
    char *session = NULL;
    bool completed = false, failed = false;
    time_t deadline = time(NULL) + TURN_TIMEOUT_S;

    while (!completed && !failed) {
        char *line = sidecar_line(&sc, deadline);
        if (line == NULL) {
            printf("\n" RED "Aura did not answer (%s)." OFF "\n",
                   time(NULL) >= deadline ? "timed out" : "sidecar exited");
            failed = true;
            break;
        }

        struct jv *fr = json_parse(line);
        if (fr == NULL)
            continue;  /* not a frame; the engine keeps stdout clean, but be lenient */

        const char *kind = jstr(fr, "kind");
        const char *method = jstr(fr, "method");
        const char *id = jstr(fr, "id");
        const struct jv *p = jget(fr, "params");

        if (kind != NULL && strcmp(kind, "res") == 0 && id != NULL) {
            const struct jv *ok = jget(fr, "ok");
            bool is_ok = ok != NULL && ok->type == J_BOOL && ok->b;
            const char *err = jstr(jget(fr, "error"), "message");

            if (strcmp(id, "1") == 0) {
                const char *sid = jstr(jget(fr, "result"), "sessionId");
                if (!is_ok || sid == NULL) {
                    printf(RED "Aura could not open a session: %s" OFF "\n", err != NULL ? err : "?");
                    failed = true;
                } else {
                    session = strdup(sid);
                    char *tp = NULL;
                    size_t tl = 0;
                    FILE *t = open_memstream(&tp, &tl);
                    fputs("{\"sessionId\":", t);
                    json_write_str(t, session);
                    fputs(",\"message\":", t);
                    json_write_str(t, prompt);
                    fputc('}', t);
                    fclose(t);
                    send_req(&sc, "2", "turn.send", tp);
                    free(tp);
                }
            } else if (strcmp(id, "2") == 0 && !is_ok) {
                printf(RED "Aura refused the question: %s" OFF "\n", err != NULL ? err : "?");
                failed = true;
            }
        } else if (kind != NULL && strcmp(kind, "req") == 0 && id != NULL) {
            /* Engine asks us something. Tools are off; deny everything. */
            if (method != NULL && strcmp(method, "approval.request") == 0) {
                const char *tool = jstr(p, "tool");
                printf("\n" DIM "[Aura asked to use %s: denied, Ask Aura is read-only]" OFF "\n",
                       tool != NULL ? tool : "a tool");
                fprintf(sc.in, "{\"kind\":\"res\",\"id\":");
                json_write_str(sc.in, id);
                fputs(",\"ok\":true,\"result\":{\"decision\":\"deny\"}}\n", sc.in);
            } else {
                fprintf(sc.in, "{\"kind\":\"res\",\"id\":");
                json_write_str(sc.in, id);
                fputs(",\"ok\":false,\"error\":{\"code\":\"unknown_method\",\"message\":\"aura-term-ask\"}}\n", sc.in);
            }
            fflush(sc.in);
        } else if (kind != NULL && strcmp(kind, "evt") == 0 && method != NULL) {
            if (strcmp(method, "turn.delta") == 0) {
                const char *text = jstr(p, "text");
                if (text != NULL) {
                    fputs(text, stdout);
                    fflush(stdout);
                    buf_add(&answer, text, strlen(text));
                }
            } else if (strcmp(method, "turn.tool_call") == 0) {
                const char *name = jstr(p, "name");
                printf("\n" DIM "[tool call: %s]" OFF "\n", name != NULL ? name : "?");
            } else if (strcmp(method, "turn.error") == 0) {
                const char *msg = jstr(p, "message");
                printf("\n" RED "Aura hit an error: %s" OFF "\n", msg != NULL ? msg : "?");
            } else if (strcmp(method, "turn.completed") == 0) {
                completed = true;
            }
        }
        jv_free(fr);
    }

    if (session != NULL) {
        /* Drop the session (and its stored history) again */
        char *dp = NULL;
        size_t dl = 0;
        FILE *d = open_memstream(&dp, &dl);
        fputs("{\"sessionId\":", d);
        json_write_str(d, session);
        fputc('}', d);
        fclose(d);
        send_req(&sc, "3", "session.destroy", dp);
        free(dp);

        time_t until = time(NULL) + 5;
        char *line;
        while ((line = sidecar_line(&sc, until)) != NULL) {
            struct jv *fr = json_parse(line);
            const char *id = jstr(fr, "id");
            bool done = id != NULL && strcmp(id, "3") == 0;
            jv_free(fr);
            if (done)
                break;
        }
        free(session);
    }

    sidecar_stop(&sc);
    free(sc.line.data);

    if (failed && answer.len == 0) {
        free(answer.data);
        return NULL;
    }
    return answer.data;
}

/* ---- main -------------------------------------------------------------- */

static char *
read_tty_line(void)
{
    FILE *tty = fopen("/dev/tty", "re");
    if (tty == NULL)
        return NULL;
    char *line = NULL;
    size_t cap = 0;
    if (getline(&line, &cap, tty) < 0) {
        free(line);
        line = NULL;
    }
    fclose(tty);
    return line;
}

int
main(int argc, char **argv)
{
    const char *input = NULL;
    bool delete_input = false, prompt_user = true;

    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--input") == 0 && i + 1 < argc)
            input = argv[++i];
        else if (strcmp(argv[i], "--delete-input") == 0)
            delete_input = true;
        else if (strcmp(argv[i], "--no-prompt") == 0)
            prompt_user = false;
        else {
            fprintf(stderr, "usage: %s [--input FILE [--delete-input]] [--no-prompt] < request.json\n", argv[0]);
            return 2;
        }
    }
    signal(SIGPIPE, SIG_IGN);

    FILE *in = input != NULL ? fopen(input, "re") : stdin;
    if (in == NULL) {
        fprintf(stderr, "aura-term-ask: %s: %s\n", input, strerror(errno));
        return 1;
    }
    char *raw = read_all(in);
    if (in != stdin)
        fclose(in);
    if (input != NULL && delete_input)
        unlink(input);

    struct jv *req = json_parse(raw);
    free(raw);
    if (req == NULL || req->type != J_OBJ ||
        (jstr(req, "selection") == NULL && jstr(req, "command") == NULL))
    {
        fprintf(stderr, "aura-term-ask: expected {\"selection\"} or {\"command\",\"output\",\"exit\",\"cwd\"} JSON\n");
        jv_free(req);
        return 1;
    }

    const char *cwd = jstr(req, "cwd");
    struct stat st;
    if (cwd == NULL || stat(cwd, &st) < 0 || !S_ISDIR(st.st_mode))
        cwd = getenv("HOME") != NULL ? getenv("HOME") : "/";

    /* Header */
    printf(BOLD "Ask Aura" OFF "  " DIM "%s" OFF "\n", cwd);
    if (jstr(req, "selection") != NULL)
        printf(DIM "about the selected text" OFF "\n\n");
    else {
        const struct jv *ev = jget(req, "exit");
        printf(DIM "$ %s  " OFF RED "(exit %d)" OFF "\n\n",
               jstr(req, "command"), ev != NULL && ev->type == J_NUM ? (int)ev->num : -1);
    }
    fflush(stdout);

    char *prompt = build_prompt(req);
    char *answer = NULL;
    int ret = 0;

    if (!in_path("aura")) {
        printf("Aura is not installed here (no `aura` in PATH), so there is no answer.\n"
               "Install aura-code to enable Ask Aura. This is what she would be asked:\n\n"
               DIM "%s" OFF "\n", prompt);
        ret = 3;
    } else {
        answer = ask_aura(cwd, prompt);
        if (answer == NULL)
            ret = 1;
        printf("\n");
    }

    char *suggestion = answer != NULL ? suggested_command(answer) : NULL;

    if (!prompt_user) {
        if (suggestion != NULL)
            printf("\nSuggested: %s\n", suggestion);
    } else if (suggestion != NULL) {
        printf("\n" BOLD "Suggested:" OFF "  %s\n\n"
               "Press Enter to run it here (in %s), or type q and Enter to close. ",
               suggestion, cwd);
        fflush(stdout);
        char *line = read_tty_line();
        if (line != NULL && (line[0] == '\n' || line[0] == '\0')) {
            printf("\n");
            fflush(stdout);
            pid_t pid = fork();
            if (pid == 0) {
                if (chdir(cwd) < 0)
                    _exit(126);
                execl("/bin/sh", "sh", "-c", suggestion, (char *)NULL);
                _exit(127);
            }
            int status = 0;
            waitpid(pid, &status, 0);
            printf("\n" DIM "[exit %d] Press Enter to close." OFF, WIFEXITED(status) ? WEXITSTATUS(status) : -1);
            fflush(stdout);
            free(read_tty_line());
        }
        free(line);
    } else {
        printf(DIM "Press Enter to close." OFF);
        fflush(stdout);
        free(read_tty_line());
    }

    free(suggestion);
    free(answer);
    free(prompt);
    jv_free(req);
    return ret;
}
