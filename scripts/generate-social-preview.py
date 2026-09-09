"""Generate the GitHub social preview image for sachncs/find.

Output: .github/social-preview.png (1200x630 PNG).

Design:
- Deep purple gradient background (project's mkdocs theme primary)
- Project name "find" + tagline
- Algorithm hint: P − V·G  (multi-variant range-splitting)
- "Research / Educational Use Only" tag
- 512-variant index visual hint (a row of small dots)
- MIT license badge
"""
from PIL import Image, ImageDraw, ImageFilter, ImageFont
import os

W, H = 1200, 630

# Brand colors (match mkdocs.yml primary deep purple + amber accent)
PRIMARY_TOP = (94, 53, 177)        # deep purple
PRIMARY_BOT = (46, 25, 95)         # darker purple
ACCENT = (255, 193, 7)             # amber
TEXT = (245, 240, 255)
MUTED = (180, 165, 210)
DIM = (140, 120, 175)
GREEN_OK = (102, 187, 106)

# Create base
img = Image.new("RGB", (W, H), PRIMARY_TOP)
# Gradient
for y in range(H):
    t = y / H
    r = int(PRIMARY_TOP[0] * (1 - t) + PRIMARY_BOT[0] * t)
    g = int(PRIMARY_TOP[1] * (1 - t) + PRIMARY_BOT[1] * t)
    b = int(PRIMARY_TOP[2] * (1 - t) + PRIMARY_BOT[2] * t)
    for x in range(W):
        img.putpixel((x, y), (r, g, b))

draw = ImageDraw.Draw(img, "RGBA")

# Find fonts (fall back to default if needed)
def get_font(size, bold=False):
    paths = [
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf" if bold else "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/Library/Fonts/Arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf" if bold else "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]
    for p in paths:
        if os.path.exists(p):
            try:
                return ImageFont.truetype(p, size)
            except OSError:
                continue
    return ImageFont.load_default()


def get_mono(size):
    paths = [
        "/System/Library/Fonts/Supplemental/Courier New.ttf",
        "/Library/Fonts/Courier New.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    ]
    for p in paths:
        if os.path.exists(p):
            try:
                return ImageFont.truetype(p, size)
            except OSError:
                continue
    return ImageFont.load_default()


font_huge = get_font(130, bold=True)
font_title = get_font(46, bold=True)
font_tag = get_font(34)
font_small = get_font(24)
font_mono = get_mono(34)
font_mono_small = get_mono(22)

# Helper to draw rounded rect
def rounded_rect(d, xy, radius, fill, outline=None, width=2):
    d.rounded_rectangle(xy, radius=radius, fill=fill, outline=outline, width=width)

# ---- Big "find" wordmark ----
title = "find"
bbox = draw.textbbox((0, 0), title, font=font_huge)
tw = bbox[2] - bbox[0]
th = bbox[3] - bbox[1]
tx = (W - tw) // 2 - 100  # offset left to leave room for alias
ty = 100
draw.text((tx, ty), title, fill=TEXT, font=font_huge)

# Subtle "secp-find" alias hint next to it (faded)
sub = "/ secp-find"
sb = draw.textbbox((0, 0), sub, font=font_tag)
sw = sb[2] - sb[0]
sx = tx + tw + 24
sy = ty + (th - (sb[3] - sb[1])) // 2 + 8
draw.text((sx, sy), sub, fill=MUTED, font=font_tag)

# ---- Tagline ----
tag1 = "Sub-second secp256k1 scalar discovery"
tag2 = "512-variant range-splitting  ·  Montgomery batch inversion"
tb1 = draw.textbbox((0, 0), tag1, font=font_title)
tb2 = draw.textbbox((0, 0), tag2, font=font_tag)
draw.text(((W - (tb1[2] - tb1[0])) // 2, 280), tag1, fill=TEXT, font=font_title)
draw.text(((W - (tb2[2] - tb2[0])) // 2, 340), tag2, fill=MUTED, font=font_tag)

# ---- Equation card (the algorithm hint) ----
card_y = 400
card_h = 100
card_x = 250
card_w = W - 2 * card_x

rounded_rect(draw, (card_x, card_y, card_x + card_w, card_y + card_h), 18, (255, 255, 255, 22), outline=(255, 255, 255, 60), width=2)

eq = "x (j·G)  =  x (P  −  V·G)"
eb = draw.textbbox((0, 0), eq, font=font_mono)
draw.text(((W - (eb[2] - eb[0])) // 2, card_y + (card_h - (eb[3] - eb[1])) // 2 - 4), eq, fill=ACCENT, font=font_mono)

# ---- 512-variant visual hint (a row of dots) ----
dots_y = 540
n_dots = 32  # symbolic; not all 512 to fit width
dot_r = 5
dot_spacing = (W - 320) // n_dots
for i in range(n_dots):
    cx = 160 + i * dot_spacing + dot_spacing // 2
    cy = dots_y
    color = ACCENT if i in (5, 12, 19, 26) else DIM
    draw.ellipse((cx - dot_r, cy - dot_r, cx + dot_r, cy + dot_r), fill=color)

dots_label = "1 of 512 variants"
db = draw.textbbox((0, 0), dots_label, font=font_small)
draw.text((160, dots_y + 14), dots_label, fill=MUTED, font=font_small)

# ---- Top-left: crate badge ----
badge_x = 40
badge_y = 40
badge = "github.com/sachncs/find"
bb = draw.textbbox((0, 0), badge, font=font_small)
rounded_rect(draw, (badge_x, badge_y, badge_x + bb[2] - bb[0] + 20, badge_y + bb[3] - bb[1] + 16), 6, (255, 255, 255, 22))
draw.text((badge_x + 10, badge_y + 6), badge, fill=TEXT, font=font_small)

# ---- Top-right: "EDUCATIONAL / RESEARCH USE ONLY" warning ----
warn_x = W - 40
warn_text = "RESEARCH / EDUCATIONAL USE ONLY"
wb = draw.textbbox((0, 0), warn_text, font=font_mono_small)
ww = wb[2] - wb[0]
wh = wb[3] - wb[1]
rounded_rect(draw, (warn_x - ww - 20, badge_y, warn_x, badge_y + wh + 16), 6, (255, 193, 7, 40), outline=(255, 193, 7, 200), width=2)
draw.text((warn_x - ww - 10, badge_y + 6), warn_text, fill=ACCENT, font=font_mono_small)

# ---- Bottom: MIT + crate.io (positioned below the dots, above the edge) ----
btm = "MIT  ·  crates.io/crates/find  ·  docs.rs/find"
btm_font = font_small
bbtm = draw.textbbox((0, 0), btm, font=btm_font)
draw.text(((W - (bbtm[2] - bbtm[0])) // 2, H - 38), btm, fill=MUTED, font=btm_font)

# Save
os.makedirs(".github", exist_ok=True)
out = ".github/social-preview.png"
img.save(out, "PNG", optimize=True)
print(f"Wrote {out} ({os.path.getsize(out)} bytes)")
