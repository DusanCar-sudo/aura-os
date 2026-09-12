#include "aura-window.h"
#include "aura-split.h"

#include <stdio.h>

#include "terminal.h"
#include "wayland.h"

void
aura_window_set_pane_tag(struct wl_window *win)
{
#if defined(HAVE_XDG_TOPLEVEL_TAG)
    struct terminal *term = win->term;
    const struct config *conf = term->conf;

    if (conf->toplevel_tag != NULL && conf->toplevel_tag[0] != '\0' &&
        !aura_split_parse_tag(win->term, conf->toplevel_tag))
        return;
    if (term->wl->toplevel_tag_manager == NULL || term->slave <= 0)
        return;

    char tag[32];
    snprintf(tag, sizeof(tag), "aura-pane-%d", term->slave);
    xdg_toplevel_tag_manager_v1_set_toplevel_tag(
        term->wl->toplevel_tag_manager, win->xdg_toplevel, tag);
    xdg_toplevel_tag_manager_v1_set_toplevel_description(
        term->wl->toplevel_tag_manager, win->xdg_toplevel, tag);
#else
    (void)win;
#endif
}
