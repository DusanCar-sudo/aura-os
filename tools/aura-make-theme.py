#!/usr/bin/env python3
"""aura-make-theme — one theme, everywhere, with a wallpaper that matches.

  tools/aura-make-theme.py import <shell-theme.conf>...   7 colours -> a full palette
  tools/aura-make-theme.py wallpaper [name]...            draw the wallpapers
  tools/aura-make-theme.py all                            import the shell themes, then draw

The settings menus (aura-os-shell) carry themes with seven colours; the
system palette in themes/<name>/palette wants fifteen. `import` fills the
missing ones from the seven, keeping each theme's own hue: brights are the
same colour lifted toward white (or ink, on a light theme), and the six
semantic colours are borrowed from the closest of our tuned sets and nudged
onto the theme's saturation, so red still reads as red without clashing.

`wallpaper` draws a 3840x2160 PNG per theme with ImageMagick: the theme
background, a soft accent glow off-centre, and a faint grid. Small files,
no photos, nothing to license.
"""
import colorsys
import os
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
THEMES = REPO / "themes"
WALLS = REPO / "assets" / "wallpapers"

KEYS = ["bg", "bg_alt", "border", "fg", "fg_bright", "dim", "muted", "accent",
        "accent_bright", "red", "green", "yellow", "blue", "cyan", "orange"]

# Semantic colours for a dark and a light theme, before the theme's own
# saturation is applied.
DARK_SEMANTIC = dict(red="#f38ba8", green="#a6e3a1", yellow="#f9e2af",
                     blue="#89b4fa", cyan="#94e2d5", orange="#fab387")
LIGHT_SEMANTIC = dict(red="#d20f39", green="#40a02b", yellow="#df8e1d",
                      blue="#1e66f5", cyan="#179299", orange="#fe640b")


def hex2rgb(h):
    h = h.strip().strip('"').lstrip("#")
    return tuple(int(h[i:i + 2], 16) / 255 for i in (0, 2, 4))


def rgb2hex(rgb):
    return "#" + "".join(f"{max(0, min(255, round(c * 255))):02x}" for c in rgb)


def mix(a, b, t):
    return tuple(x + (y - x) * t for x, y in zip(a, b))


def luma(rgb):
    r, g, b = rgb
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def with_saturation(rgb, sat):
    h, l, s = colorsys.rgb_to_hls(*rgb)
    return colorsys.hls_to_rgb(h, l, max(0.25, min(1.0, sat)))


def read_conf(path):
    """The shell theme format: KEY="#rrggbb" lines."""
    out = {}
    for line in Path(path).read_text().splitlines():
        m = re.match(r'^\s*([A-Z_0-9]+)\s*=\s*"?([^"\n]*)"?\s*$', line)
        if m:
            out[m.group(1)] = m.group(2)
    return out


def import_theme(conf_path):
    conf = read_conf(conf_path)
    name = Path(conf_path).stem
    need = ["BG", "SURFACE", "LINE", "FG", "DIM", "ACCENT", "ACCENT2"]
    if not all(k in conf for k in need):
        print(f"{name}: missing {[k for k in need if k not in conf]}", file=sys.stderr)
        return None

    bg, surface, line = (hex2rgb(conf[k]) for k in ("BG", "SURFACE", "LINE"))
    fg, dim = hex2rgb(conf["FG"]), hex2rgb(conf["DIM"])
    accent, accent2 = hex2rgb(conf["ACCENT"]), hex2rgb(conf["ACCENT2"])
    dark = luma(bg) < 0.5
    toward = (1.0, 1.0, 1.0) if dark else (0.0, 0.0, 0.0)

    sat = colorsys.rgb_to_hls(*accent)[2]
    semantic = DARK_SEMANTIC if dark else LIGHT_SEMANTIC
    values = {
        "bg": bg,
        "bg_alt": surface,
        "border": line,
        "fg": fg,
        "fg_bright": mix(fg, toward, 0.35),
        "dim": dim,
        "muted": mix(dim, fg, 0.45),
        "accent": accent,
        "accent_bright": mix(accent, toward, 0.3),
    }
    for key, hexv in semantic.items():
        values[key] = with_saturation(hex2rgb(hexv), sat)
    # the theme's own second colour is more its own than a borrowed cyan
    values["cyan"] = accent2

    title = conf.get("NAME", name).strip('"')
    body = [f"# {name} — {title}, imported from the settings menus' theme"]
    body += [f"{k}={rgb2hex(values[k])}" for k in KEYS]
    out = THEMES / name / "palette"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text("\n".join(body) + "\n")
    print(f"wrote {out.relative_to(REPO)}")
    return name


def palette(name):
    text = (THEMES / name / "palette").read_text()
    return dict(re.findall(r"^([a-z_]+)=(#[0-9a-fA-F]{6})$", text, re.M))


def wallpaper(name, width=2560, height=1440):
    """Theme background, two soft glows, a faint grid.

    Each glow is a gradient drawn at twice the size and then cropped, so
    what lands on the picture is the middle of a smooth falloff — pasting a
    smaller gradient on top leaves a visible rectangle edge instead.
    Gradients scale well, so 2560x1440 fills a 4K screen without a seam.
    """
    p = palette(name)
    bg, accent, alt = p["bg"], p["accent"], p.get("bg_alt", p["bg"])
    dark = luma(hex2rgb(bg)) < 0.5
    grid_alpha = 0.05 if dark else 0.09
    step = 96
    big = f"{width * 2}x{height * 2}"

    def glow(colour, alpha, x, y):
        """A gradient window cropped out of a double-size radial gradient."""
        return [
            "(", "-size", big, f"radial-gradient:{colour}-{bg}",
            "-crop", f"{width}x{height}+{x}+{y}", "+repage",
            "-alpha", "set", "-channel", "A", "-evaluate", "multiply", str(alpha), "+channel", ")",
            "-compose", "over", "-composite",
        ]

    WALLS.mkdir(parents=True, exist_ok=True)
    out = WALLS / f"{name}.png"
    cmd = ["magick", "-size", f"{width}x{height}", f"xc:{bg}"]
    # the accent glow up and right of centre, a cooler one low and left
    cmd += glow(accent, 0.22, int(width * 0.75), int(height * 0.35))
    cmd += glow(alt, 0.30, int(width * 0.30), int(height * 0.95))
    # the grid: one tile, drawn once and tiled over everything
    cmd += [
        "(", "-size", f"{step}x{step}", "xc:none",
        "-fill", "none", "-stroke", accent, "-strokewidth", "1",
        "-draw", f"line 0,0 {step},0", "-draw", f"line 0,0 0,{step}",
        "-alpha", "set", "-channel", "A", "-evaluate", "multiply", str(grid_alpha), "+channel",
        "-write", "mpr:tile", "+delete",
        "-size", f"{width}x{height}", "tile:mpr:tile", ")",
        "-compose", "over", "-composite",
        "-define", "png:compression-level=9", str(out),
    ]
    subprocess.run(cmd, check=True)
    print(f"wrote {out.relative_to(REPO)}  ({out.stat().st_size // 1024} KB)")


def main(argv):
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__)
        return 0
    cmd, rest = argv[0], argv[1:]
    shell_repo = Path(os.environ.get("AURA_OS_SHELL_REPO", "/mnt/bigdata/aura-os-shell"))
    if cmd in ("import", "all"):
        confs = [Path(p) for p in rest] or sorted((shell_repo / "themes").glob("*.conf"))
        for c in confs:
            if not (THEMES / c.stem / "palette").exists():
                import_theme(c)
            else:
                print(f"{c.stem}: already a system theme, left alone")
    if cmd in ("wallpaper", "all"):
        names = rest if cmd == "wallpaper" and rest else sorted(d.name for d in THEMES.iterdir() if (d / "palette").exists())
        for n in names:
            wallpaper(n)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
