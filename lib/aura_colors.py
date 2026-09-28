"""Shared curses theming for the Aura TUI apps (aura-files, aura-edit).

Reads the same theme conf the rest of the shell uses (active theme in
~/.config/aura/theme, fallback themes/ next to this checkout), so switching
theme in aura-theme repaints these apps too — including the light ones.

If the terminal can't redefine colors (8/16-color terms), fall back to
nearest ANSI colors instead of failing.
"""
import os

# color indexes we own (16+ leaves the standard 16 untouched)
_IDX = {"BG": 16, "SURFACE": 17, "FG": 18, "DIM": 19, "ACCENT": 20, "ACCENT2": 21}


def _share_dir():
    return os.path.dirname(os.path.dirname(os.path.realpath(__file__)))

def _config_dir():
    return os.path.join(os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config")), "aura")


def theme_name():
    """Active theme: whichever theme system was used last (aura-os-shell
    writes ~/.config/aura/theme, aura-os-work writes ~/.config/aura-os/theme
    — users can hit either). Returns (name, system)."""
    shell = os.path.join(_config_dir(), "theme")
    work = os.path.join(os.environ.get("XDG_CONFIG_HOME",
                                       os.path.expanduser("~/.config")), "aura-os", "theme")
    try:
        s_mtime = os.stat(shell).st_mtime
    except OSError:
        s_mtime = -1
    try:
        w_mtime = os.stat(work).st_mtime
    except OSError:
        w_mtime = -1
    try:
        with open(shell if s_mtime >= w_mtime else work) as f:
            return (f.read().strip() or "neon-night",
                    "shell" if s_mtime >= w_mtime else "work")
    except OSError:
        return ("neon-night", "shell")


def parse_theme(name=None):
    """Load the active theme into uppercase ROLE=#hex, from either system."""
    name, system = theme_name()
    vals = {}
    if system == "shell":
        # the shell checkout sits next to this repo on disk (bigdata), and
        # AURA_SHARE overrides for non-standard installs
        shell_dirs = [os.environ.get("AURA_SHARE"),
                      os.path.join(os.path.dirname(_share_dir()), "aura-os-shell"),
                      _share_dir()]
        conf = "%s.conf" % name
        paths = [os.path.join(d, "themes", conf)
                 for d in shell_dirs if d and os.path.isfile(os.path.join(d, "themes", conf))]
        fallback = os.path.join(shell_dirs[1], "themes", "neon-night.conf")
    else:
        # aura-os-work: themes/<name>/palette, lowercase keys
        for d in (os.path.join(_config_dir(), "..", "aura-os", "themes"),
                  "/usr/share/aura-os/themes",
                  os.path.join(_share_dir(), "themes")):
            p = os.path.join(d, name, "palette")
            if os.path.isfile(p):
                paths = [p]
                break
        fallback = os.path.join(_share_dir(), "themes", "neon-night", "palette")
    if not paths or not os.path.isfile(paths[0]):
        paths = [fallback if os.path.isfile(fallback)
                 else paths[0] if paths else fallback]

    _PALETTE_MAP = {"bg": "BG", "bg_alt": "SURFACE", "border": "LINE", "fg": "FG",
                    "dim": "DIM", "muted": "DIM", "accent": "ACCENT",
                    "accent_bright": "ACCENT2", "cyan": "ACCENT2"}
    for path in paths:
        try:
            with open(path) as f:
                for line in f:
                    line = line.strip()
                    if "=" in line and not line.startswith("#"):
                        k, _, v = line.partition("=")
                        k, v = k.strip(), v.strip().strip('"').strip("'")
                        if k.isupper():
                            vals[k] = v
                        elif k in _PALETTE_MAP:
                            vals.setdefault(_PALETTE_MAP[k], v)
        except OSError:
            continue
        if vals:
            break
    for k in ("BG", "SURFACE", "LINE", "FG", "DIM", "ACCENT", "ACCENT2"):
        vals.setdefault(k, "#000000" if k == "BG" else "#ffffff")
    return vals


def _hex(v, default):
    v = v.strip()
    if len(v) == 7 and v[0] == "#":
        try:
            return tuple(int(v[i:i + 2], 16) for i in (1, 3, 5))
        except ValueError:
            pass
    return default


def _luminance(rgb):
    r, g, b = rgb
    return 0.299 * r + 0.587 * g + 0.114 * b


def _nearest_ansi(rgb):
    r, g, b = rgb
    names = [("black", 0, 0, 0), ("red", 205, 0, 0), ("green", 0, 205, 0),
             ("yellow", 205, 205, 0), ("blue", 0, 0, 238), ("magenta", 205, 0, 205),
             ("cyan", 0, 205, 205), ("white", 229, 229, 229)]
    return min(names, key=lambda n: (n[1] - r) ** 2 + (n[2] - g) ** 2 + (n[3] - b) ** 2)[0]


def setup(curses, order=("text", "accent", "bar", "current", "select", "dim", "alt", "body")):
    """Init curses colors from the active theme. Returns {pair_number: role}
    for the roles requested, defined as pairs 1..len(order) — callers keep
    using their existing pair numbers, they just come out theme-colored now.
    """
    t = parse_theme()
    roles = {r: _hex(t[k], (255,) * 3 if k == "FG" else (0,) * 3)
             for r, k in (("bg", "BG"), ("surface", "SURFACE"), ("fg", "FG"),
                          ("dim", "DIM"), ("accent", "ACCENT"), ("accent2", "ACCENT2"))}

    can_redefine = (curses.COLORS >= 256 and curses.can_change_color()
                    and os.environ.get("TERM") not in ("linux",))
    if can_redefine:
        for role, idx in _IDX.items():
            r, g, b = roles[role.lower()]
            curses.init_color(idx, r * 1000 // 255, g * 1000 // 255, b * 1000 // 255)
        c = {r.lower(): i for r, i in _IDX.items()}
    else:
        # 8/16-color fallback: nearest ANSI for fg-ish roles; the background
        # can only be black/white/default, so pick by theme brightness.
        c = {}
        for role in roles:
            name = _nearest_ansi(roles[role])
            c[role] = getattr(curses, "COLOR_" + name.upper())
        c["bg"] = curses.COLOR_WHITE if _luminance(roles["bg"]) > 128 else curses.COLOR_BLACK
        c["fg"] = curses.COLOR_BLACK if c["bg"] == curses.COLOR_WHITE else curses.COLOR_WHITE

    pairs = {
        "text":    (c["fg"], c["bg"]),
        "accent":  (c["accent"], c["bg"]),      # folders, path, places, bars
        "bar":     (c["bg"], c["accent"]),      # toolbar, dialogs, active menu
        "current": (c["bg"], c["accent2"]),     # current item / buttons
        "select":  (c["accent2"], c["bg"]),     # selected, code, archives
        "dim":     (c["dim"], c["bg"]),
        "alt":     (c["accent2"], c["bg"]),     # media, errors, executables
        "body":    (c["fg"], c["surface"]),     # menus / panels
    }
    numbers = {}
    for n, role in enumerate(order, start=1):
        fg, bg = pairs[role]
        curses.init_pair(n, fg, bg)
        numbers[n] = role
    return numbers


def paint_base(scr, curses):
    """Fill the screen with the theme background (curses apps must do this
    themselves or a light theme shows as a dark window in a dark terminal)."""
    scr.bkgd(" ", curses.color_pair(1))
    scr.clear()
