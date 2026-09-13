#!/usr/bin/env python3
"""make-splash — render the Aura OS boot splash (80s horizon look).

    tools/make-splash.py VARIANT [--out FILE] [--size 1920x1200]
    tools/make-splash.py all --out-dir DIR      render every variant + a contact sheet

Static image: the UKI shows it while the kernel loads (install/08-boot.sh,
mkinitcpio --splash), so it must be a 24-bit BMP. PNG is written too for
previews. Fonts come from assets/fonts (OFL, see THIRD-PARTY-NOTICES.md).
"""
import math
import os
import random
import sys

from PIL import Image, ImageDraw, ImageFilter, ImageFont

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FONT_DIRS = [os.path.join(REPO, 'assets', 'fonts'),
             os.path.expanduser('~/.local/share/aura-splash/fonts')]


def font(name, size):
    for d in FONT_DIRS:
        for f in os.listdir(d) if os.path.isdir(d) else []:
            if f.lower().startswith(name.lower()):
                ft = ImageFont.truetype(os.path.join(d, f), size)
                if name.lower() == 'orbitron':
                    try:
                        ft.set_variation_by_axes([800])
                    except Exception:
                        pass
                return ft
    raise SystemExit(f'font {name} not found in {FONT_DIRS}')


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def hexc(h):
    h = h.lstrip('#')
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


SKY_TOP, SKY_LOW = hexc('#05040b'), hexc('#1c0930')
SUN_TOP, SUN_MID, SUN_LOW = hexc('#ffc46b'), hexc('#ff4f8b'), hexc('#a33cff')
GRID, HORIZON = hexc('#3d7bff'), hexc('#ff3fa4')
CREAM, VIOLET, DIM = hexc('#f6e3d0'), hexc('#b58cff'), hexc('#8a86a8')


# ---------------------------------------------------------------- scene
def scene(W, H, horizon=0.56, sun_r=0.17, sun_x=0.5, grid=True, stars=True):
    img = Image.new('RGB', (W, H))
    d = ImageDraw.Draw(img)
    hy = int(H * horizon)
    for y in range(hy):
        d.line([(0, y), (W, y)], fill=lerp(SKY_TOP, SKY_LOW, (y / hy) ** 1.8))
    for y in range(hy, H):
        d.line([(0, y), (W, y)], fill=lerp(hexc('#0d0518'), SKY_TOP, (y - hy) / (H - hy)))

    if stars:
        rnd = random.Random(7)
        for _ in range(140):
            x, y = rnd.randrange(W), rnd.randrange(int(hy * 0.8))
            v = rnd.randrange(60, 190)
            d.point((x, y), fill=(v, v, min(255, v + 40)))

    # sun: gradient disc with widening cut-out stripes, sitting on the horizon
    r = int(H * sun_r)
    cx, cy = int(W * sun_x), hy - int(r * 0.35)
    sun = Image.new('RGB', (2 * r, 2 * r))
    sd = ImageDraw.Draw(sun)
    for y in range(2 * r):
        t = y / (2 * r)
        c = lerp(SUN_TOP, SUN_MID, t / 0.55) if t < 0.55 else lerp(SUN_MID, SUN_LOW, (t - 0.55) / 0.45)
        sd.line([(0, y), (2 * r, y)], fill=c)
    mask = Image.new('L', (2 * r, 2 * r), 0)
    md = ImageDraw.Draw(mask)
    md.ellipse([0, 0, 2 * r - 1, 2 * r - 1], fill=255)
    y, gap, k = int(r * 0.95), max(2, r // 60), 0
    while y < 2 * r:
        md.rectangle([0, y, 2 * r, y + gap], fill=0)
        k += 1
        y += int(r * 0.17) - k * max(1, r // 90)
        gap += max(1, r // 70)
        if int(r * 0.17) - k * max(1, r // 90) < gap + 4:
            break
    # glow behind the sun
    glow = Image.new('RGB', (W, H), (0, 0, 0))
    gd = ImageDraw.Draw(glow)
    gd.ellipse([cx - r * 1.25, cy - r * 1.25, cx + r * 1.25, cy + r * 1.25], fill=(120, 20, 90))
    glow = glow.filter(ImageFilter.GaussianBlur(r * 0.45))
    img = Image.composite(img, img, Image.new('L', (W, H), 255))
    img = add(img, glow, hy)
    img.paste(sun, (cx - r, cy - r), mask)
    # hide the part of the sun below the horizon
    d = ImageDraw.Draw(img)
    for yy in range(hy, H):
        d.line([(0, yy), (W, yy)], fill=lerp(hexc('#0d0518'), SKY_TOP, (yy - hy) / (H - hy)))

    if grid:
        g = Image.new('RGB', (W, H), (0, 0, 0))
        gd = ImageDraw.Draw(g)
        vx = W / 2
        for i in range(-26, 27):
            x_bottom = vx + i * W * 0.085
            gd.line([(vx + i * W * 0.004, hy), (x_bottom, H)], fill=GRID, width=2)
        for j in range(1, 24):
            z = j / 23
            y = hy + (H - hy) * (z ** 2.4)
            fade = 0.25 + 0.75 * z
            gd.line([(0, y), (W, y)], fill=tuple(int(c * fade) for c in GRID), width=2)
        # fade the grid into the horizon haze
        fm = Image.new('L', (W, H), 0)
        fd = ImageDraw.Draw(fm)
        for y in range(hy, H):
            fd.line([(0, y), (W, y)], fill=int(255 * min(1, ((y - hy) / (H - hy)) ** 0.6 * 1.6)))
        g = Image.composite(g, Image.new('RGB', (W, H)), fm)
        img = add(img, g.filter(ImageFilter.GaussianBlur(6)), 0)
        img = add(img, g, 0)

    # horizon line + haze
    hl = Image.new('RGB', (W, H))
    hd = ImageDraw.Draw(hl)
    hd.line([(0, hy), (W, hy)], fill=HORIZON, width=3)
    img = add(img, hl.filter(ImageFilter.GaussianBlur(10)), 0)
    img = add(img, hl, 0)
    return img, hy, (cx, cy, r)


def add(a, b, _):
    from PIL import ImageChops
    return ImageChops.add(a, b)


# ---------------------------------------------------------------- text
def spaced(d, xy, text, ft, fill, spacing=0.0, anchor='mm'):
    """draw letter-spaced text centered (anchor mm) or left (lm)."""
    widths = [d.textlength(ch, font=ft) for ch in text]
    extra = ft.size * spacing
    total = sum(widths) + extra * (len(text) - 1)
    x, y = xy
    if anchor == 'mm':
        x -= total / 2
    elif anchor == 'rm':
        x -= total
    for ch, w in zip(text, widths):
        d.text((x, y), ch, font=ft, fill=fill, anchor='lm')
        x += w + extra
    return total


def glow_text(img, xy, text, ft, fill, spacing=0.0, anchor='mm', blur=14, glow=None):
    W, H = img.size
    layer = Image.new('RGB', (W, H))
    spaced(ImageDraw.Draw(layer), xy, text, ft, glow or fill, spacing, anchor)
    img = add(img, layer.filter(ImageFilter.GaussianBlur(blur)), 0)
    spaced(ImageDraw.Draw(img), xy, text, ft, fill, spacing, anchor)
    return img


def chrome_text(img, xy, text, ft, spacing=0.06):
    """80s chrome: cream-to-blue vertical gradient, pink rim glow."""
    W, H = img.size
    m = Image.new('L', (W, H), 0)
    spaced(ImageDraw.Draw(m), xy, text, ft, 255, spacing)
    box = m.getbbox()
    if not box:
        return img
    x0, y0, x1, y1 = box
    grad = Image.new('RGB', (W, H))
    gd = ImageDraw.Draw(grad)
    stops = [(0, hexc('#ffffff')), (0.45, hexc('#bfe3ff')), (0.52, hexc('#3a2a8a')),
             (0.7, hexc('#ff7ad9')), (1, hexc('#ffe1f4'))]
    for y in range(y0, y1 + 1):
        t = (y - y0) / max(1, y1 - y0)
        for (ta, ca), (tb, cb) in zip(stops, stops[1:]):
            if ta <= t <= tb:
                gd.line([(x0, y), (x1, y)], fill=lerp(ca, cb, (t - ta) / (tb - ta)))
                break
    rim = Image.new('RGB', (W, H))
    rim.paste(HORIZON, mask=m.filter(ImageFilter.MaxFilter(9)))
    img = add(img, rim.filter(ImageFilter.GaussianBlur(18)), 0)
    img.paste(grad, (0, 0), m)
    return img


# ---------------------------------------------------------------- variants
BOOT = ['> Booting Aura OS ...', '> Starting Aura Code agent ...', '> Welcome, builder.']


def v_horizon(W, H, title='AURA OS 1', tag='AGENT DRIVEN OPERATING SYSTEM', boot=True, corner='From ideas to reality.'):
    """title on top, sun on the grid, boot log under the horizon (ref #2 right)."""
    img, hy, _ = scene(W, H, horizon=0.5, sun_r=0.13)
    img = glow_text(img, (W / 2, H * 0.17), title, font('Michroma', int(H * 0.055)), CREAM, 0.55, blur=10, glow=VIOLET)
    if tag:
        img = glow_text(img, (W / 2, H * 0.245), tag, font('ShareTech', int(H * 0.022)), VIOLET, 0.35, blur=6)
    d = ImageDraw.Draw(img)
    if boot:
        mono = font('ShareTech', int(H * 0.026))
        for i, line in enumerate(BOOT):
            d.text((W * 0.33, H * 0.68 + i * H * 0.045), line, font=mono, fill=CREAM if i < 2 else VIOLET, anchor='lm')
        d.rectangle([W * 0.33, H * 0.68 + 3 * H * 0.045 - 4, W * 0.33 + H * 0.02, H * 0.68 + 3 * H * 0.045 + 4], fill=CREAM)
    if corner:
        d.text((W * 0.97, H * 0.95), corner, font=font('ShareTech', int(H * 0.018)), fill=DIM, anchor='rm')
    return img


def v_chrome(W, H, title='AURA OS 1', tag='AGENT DRIVEN OPERATING SYSTEM', side=('SAME TOOLS', 'NEW POSSIBILITIES'), bar=True):
    """big chrome title across the sun, loading bar (ref #9)."""
    img, hy, (cx, cy, r) = scene(W, H, horizon=0.6, sun_r=0.24)
    img = chrome_text(img, (W / 2, hy - H * 0.06), title, font('Audiowide', int(H * 0.13)), 0.04)
    img = glow_text(img, (W / 2, hy + H * 0.06), tag, font('Michroma', int(H * 0.026)), CREAM, 0.3, blur=8, glow=HORIZON)
    d = ImageDraw.Draw(img)
    if side:
        sm = font('ShareTech', int(H * 0.02))
        for i, s in enumerate(side):
            d.text((W * 0.94, H * 0.1 + i * H * 0.04), s, font=sm, fill=CREAM, anchor='rm')
        d.line([(W * 0.94 - H * 0.04, H * 0.1 + 2.4 * H * 0.04), (W * 0.94, H * 0.1 + 2.4 * H * 0.04)], fill=VIOLET, width=3)
    if bar:
        bw, by = W * 0.26, H * 0.83
        d.rectangle([W / 2 - bw / 2, by - 5, W / 2 + bw / 2, by + 5], fill=hexc('#2a1840'))
        grad_w = int(bw * 0.62)
        for x in range(grad_w):
            d.line([(W / 2 - bw / 2 + x, by - 5), (W / 2 - bw / 2 + x, by + 5)], fill=lerp(HORIZON, SUN_TOP, x / grad_w))
        d.text((W / 2, by + H * 0.045), 'Loading agents ...', font=font('ShareTech', int(H * 0.022)), fill=CREAM, anchor='mm')
    return img


def logo(W, H, cx, cy, r):
    """the mark: striped sunset disc with an 'A' peak cut out of it (ref #2 left)."""
    img = Image.new('RGB', (W, H))
    sun = Image.new('RGB', (2 * r, 2 * r))
    sd = ImageDraw.Draw(sun)
    for y in range(2 * r):
        t = y / (2 * r)
        c = lerp(SUN_TOP, SUN_MID, t / 0.5) if t < 0.5 else lerp(SUN_MID, SUN_LOW, (t - 0.5) / 0.5)
        sd.line([(0, y), (2 * r, y)], fill=c)
    m = Image.new('L', (2 * r, 2 * r), 0)
    md = ImageDraw.Draw(m)
    md.ellipse([0, 0, 2 * r - 1, 2 * r - 1], fill=255)
    for k in range(9):
        y = int(r * (0.62 + k * 0.155))
        md.rectangle([0, y, 2 * r, y + max(3, r // 40)], fill=0)
    # A: a dark triangle from the bottom, with a small arch cut
    md.polygon([(r, int(r * 0.62)), (int(r * 0.42), 2 * r), (int(r * 1.58), 2 * r)], fill=0)
    md.pieslice([int(r * 0.78), int(r * 1.38), int(r * 1.22), int(r * 1.82)], 180, 360, fill=255)
    md.rectangle([0, int(r * 1.88), 2 * r, 2 * r], fill=0)
    img.paste(sun, (cx - r, cy - r), m)
    return img, m


def v_mark(W, H, title='AURA', sub='adOS', tag='AGENT DRIVEN OPERATING SYSTEM', words='CODE   RUN   BUILD   FREEDOM'):
    """logo mark left, name right — the 'AURA adOS' identity (ref #2 left)."""
    img, hy, _ = scene(W, H, horizon=0.8, sun_r=0.001, grid=True)
    r = int(H * 0.22)
    lx, ly = int(W * 0.29), int(H * 0.44)
    halo = Image.new('RGB', (W, H))
    ImageDraw.Draw(halo).ellipse([lx - r * 1.2, ly - r * 1.2, lx + r * 1.2, ly + r * 1.2], fill=(110, 20, 80))
    img = add(img, halo.filter(ImageFilter.GaussianBlur(r * 0.4)), 0)
    lg, m = logo(W, H, lx, ly, r)
    img.paste(lg.crop((lx - r, ly - r, lx + r, ly + r)), (lx - r, ly - r), m)
    tx = W * 0.5
    big = font('Audiowide', int(H * 0.14))
    d = ImageDraw.Draw(img)
    w = spaced(d, (tx, H * 0.39), title, big, CREAM, 0.04, anchor='lm')
    img = glow_text(img, (tx + w + H * 0.03, H * 0.39 + H * 0.012), sub, font('Audiowide', int(H * 0.075)), HORIZON, 0.02, anchor='lm', blur=12)
    img = glow_text(img, (tx + 4, H * 0.5), tag, font('Michroma', int(H * 0.021)), VIOLET, 0.28, anchor='lm', blur=6)
    ImageDraw.Draw(img).text((tx + 4, H * 0.57), words, font=font('ShareTech', int(H * 0.024)), fill=DIM, anchor='lm')
    return img


def v_minimal(W, H, title='AURA OS', tag="It's better."):
    """quiet: small mark centered, name under it, no grid — calm, fast to read."""
    img = Image.new('RGB', (W, H), hexc('#0e0e14'))
    r = int(H * 0.11)
    cx, cy = W // 2, int(H * 0.4)
    halo = Image.new('RGB', (W, H))
    ImageDraw.Draw(halo).ellipse([cx - r * 1.3, cy - r * 1.3, cx + r * 1.3, cy + r * 1.3], fill=(70, 15, 60))
    img = add(img, halo.filter(ImageFilter.GaussianBlur(r * 0.5)), 0)
    lg, m = logo(W, H, cx, cy, r)
    img.paste(lg.crop((cx - r, cy - r, cx + r, cy + r)), (cx - r, cy - r), m)
    img = glow_text(img, (W / 2, H * 0.6), title, font('Michroma', int(H * 0.045)), CREAM, 0.6, blur=8, glow=VIOLET)
    ImageDraw.Draw(img).text((W / 2, H * 0.67), tag, font=font('ShareTech', int(H * 0.026)), fill=DIM, anchor='mm')
    return img


VARIANTS = {
    'a-horizon':      lambda W, H: v_horizon(W, H),
    'b-chrome':       lambda W, H: v_chrome(W, H),
    'c-mark-ados':    lambda W, H: v_mark(W, H),
    'd-minimal':      lambda W, H: v_minimal(W, H),
    'e-chrome-better': lambda W, H: v_chrome(W, H, title='AURA OS', tag="IT'S BETTER. AGENT DRIVEN.",
                                             side=('YOUR LAPTOP', 'YOUR AGENTS'), bar=True),
    'f-horizon-ados': lambda W, H: v_horizon(W, H, title='AURA  adOS  1', tag='AGENT DRIVEN OPERATING SYSTEM',
                                             boot=True, corner='Code · Run · Build · Freedom'),
}


def save(img, path):
    img.save(path + '.png')
    img.convert('RGB').save(path + '.bmp')


def main():
    a = sys.argv[1:]
    if not a or a[0] in ('-h', '--help'):
        print(__doc__ + '\nvariants: ' + ' '.join(VARIANTS))
        return
    size = next((a[i + 1] for i, x in enumerate(a) if x == '--size'), '1920x1200')
    W, H = map(int, size.split('x'))
    if a[0] == 'all':
        out = next((a[i + 1] for i, x in enumerate(a) if x == '--out-dir'), 'splash-out')
        os.makedirs(out, exist_ok=True)
        thumbs = []
        for name, fn in VARIANTS.items():
            img = fn(W, H)
            save(img, os.path.join(out, name))
            thumbs.append((name, img.resize((W // 3, H // 3), Image.LANCZOS)))
            print('rendered', name)
        cols, tw, th = 2, W // 3, H // 3
        sheet = Image.new('RGB', (cols * tw + 30, ((len(thumbs) + 1) // 2) * (th + 40) + 10), (20, 20, 28))
        sd = ImageDraw.Draw(sheet)
        for i, (name, t) in enumerate(thumbs):
            x, y = 10 + (i % cols) * (tw + 10), 10 + (i // cols) * (th + 40)
            sheet.paste(t, (x, y + 28))
            sd.text((x, y + 4), name, font=font('ShareTech', 22), fill=CREAM)
        sheet.save(os.path.join(out, 'contact-sheet.png'))
        print('sheet', os.path.join(out, 'contact-sheet.png'))
        return
    fn = VARIANTS.get(a[0]) or sys.exit(f'unknown variant {a[0]}; have: {" ".join(VARIANTS)}')
    out = next((a[i + 1] for i, x in enumerate(a) if x == '--out'), 'splash')
    save(fn(W, H), out.rsplit('.', 1)[0])


if __name__ == '__main__':
    main()
