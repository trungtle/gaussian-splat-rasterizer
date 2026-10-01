#!/usr/bin/env python3
"""Render every debug view headless and compose an annotated comparison grid.

Each tile gets a header (viewer key + view name), a caption saying what to
look for, numbered callouts on the features the caption mentions, and a
color legend for the colormapped views.

Callout positions are normalized image coordinates for the default scene and
camera (bonsai, dataset camera 0); pass --no-callouts for other scenes.

usage: python scripts/make_view_grid.py [--ply PLY] [--cam N] [--width W]
                                        [--out PNG] [--no-callouts]
requires: Pillow, and a release build (cargo build --release)
"""
import argparse
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
EXE = ROOT / "target" / "release" / ("gaussian-splat-rasterizer.exe" if sys.platform == "win32" else "gaussian-splat-rasterizer")

# (key, view, caption, legend, callouts[(x, y, label)])
VIEWS = [
    ("1", "color", "Final render: SH degree 3, back-to-front alpha blending.", None, []),
    ("2", "base-color", "SH band 0 only. Nearly identical: most appearance is view-independent.", None, []),
    ("3", "view-dependent", "SH bands 1-3 around mid-grey: highlights that move as you orbit.", None,
     [(0.30, 0.66, "1"), (0.62, 0.80, "2")]),
    ("4", "depth", "Expected depth (log scale). Background is semi-transparent smears.", "depth",
     [(0.60, 0.42, "1"), (0.12, 0.18, "2")]),
    ("5", "opacity", "Opacity per splat, drawn opaque. Faint blue haze = floaters.", "opacity",
     [(0.22, 0.30, "1"), (0.70, 0.30, "2")]),
    ("6", "ellipses", "1-sigma / 2-sigma outlines: small on detail, huge on flat background.", None,
     [(0.14, 0.22, "1"), (0.58, 0.20, "2")]),
    ("7", "centers", "Splat centers only: the point cloud the Gaussians grew from.", None,
     [(0.07, 0.45, "1"), (0.58, 0.20, "2")]),
    ("8", "anisotropy", "Major/minor axis ratio. Needles align with thin structures.", "anisotropy",
     [(0.62, 0.47, "1"), (0.62, 0.80, "2")]),
    ("9", "overdraw", "Fragments per pixel = fill cost. Bright = where frame time goes.", "overdraw",
     [(0.62, 0.74, "1")]),
    ("0", "quads", "The 3-sigma quads actually rasterized; some span much of the screen.", None,
     [(0.16, 0.36, "1")]),
]

# Second caption line explaining the numbered callouts.
CALLOUT_NOTES = {
    "view-dependent": "(1) wheel rim and spokes: strong specular  (2) cloth: grey = no view dependence",
    "depth": "(1) bonsai + pot: near, crisp  (2) wall: depth smeared across layers",
    "opacity": "(1) floaters in front of the camera  (2) opaque (red) splats on the flowers",
    "ellipses": "(1) huge background splats  (2) tiny splats on the flowers",
    "centers": "(1) gaps between centers: the Gaussians' extent fills them  (2) dense on the bonsai",
    "anisotropy": "(1) wooden stand: elongated needles (red)  (2) cloth: round blobs (blue)",
    "overdraw": "(1) cloth and table: 50+ blended layers",
    "quads": "(1) background splats with screen-sized quads",
}

HEADER_H, CAPTION_H, LEGEND_H, PAD = 34, 50, 26, 10
BG, FG, MUTED, ACCENT = (24, 24, 28), (235, 235, 235), (170, 170, 178), (255, 214, 10)


def font(size, bold=False):
    names = (["segoeuib.ttf", "arialbd.ttf", "DejaVuSans-Bold.ttf"] if bold
             else ["segoeui.ttf", "arial.ttf", "DejaVuSans.ttf"])
    for n in names:
        for d in ["C:/Windows/Fonts", "/usr/share/fonts/truetype/dejavu", "/Library/Fonts"]:
            p = Path(d) / n
            if p.exists():
                return ImageFont.truetype(str(p), size)
    return ImageFont.load_default(size=size)


def turbo(x):
    """Same polynomial Turbo fit as splat.slang."""
    x = min(max(x, 0.0), 1.0)
    v4 = (1.0, x, x * x, x * x * x)
    v2 = (v4[2] * v4[2], v4[3] * v4[2])
    k = {
        "r": ((0.13572138, 4.61539260, -42.66032258, 132.13108234), (-152.94239396, 59.28637943)),
        "g": ((0.09140261, 2.19418839, 4.84296658, -14.18503333), (4.27729857, 2.82956604)),
        "b": ((0.10667330, 12.64194608, -60.58204836, 110.36276771), (-89.90310912, 27.34824973)),
    }
    out = []
    for c in "rgb":
        a, b = k[c]
        val = sum(p * q for p, q in zip(v4, a)) + sum(p * q for p, q in zip(v2, b))
        out.append(int(min(max(val, 0.0), 1.0) * 255))
    return tuple(out)


def overdraw_color(t):
    """Additive ramp from splat.slang: n layers of (1/48, 1/160, 1/600), t in [0,1] = 0..64 layers."""
    n = t * 64
    return tuple(int(min(n * s, 1.0) * 255) for s in (1 / 48, 1 / 160, 1 / 600))


LEGENDS = {
    "depth": (lambda t: turbo(t), "far", "near"),
    "opacity": (lambda t: turbo(t), "opacity 0", "1"),
    "anisotropy": (lambda t: turbo(t), "round 1:1", "10:1 needle"),
    "overdraw": (overdraw_color, "0 layers", "64+"),
}


def draw_legend(d, x, y, w, kind, f):
    ramp, lo, hi = LEGENDS[kind]
    lw = d.textlength(lo, font=f)
    hw = d.textlength(hi, font=f)
    bx0, bx1 = x + lw + 8, x + w - hw - 8
    for i in range(int(bx1 - bx0)):
        d.line([(bx0 + i, y + 6), (bx0 + i, y + 18)], fill=ramp(i / max(bx1 - bx0 - 1, 1)))
    d.rectangle([bx0, y + 6, bx1, y + 18], outline=MUTED)
    d.text((x, y + 3), lo, font=f, fill=MUTED)
    d.text((bx1 + 8, y + 3), hi, font=f, fill=MUTED)


def draw_callout(d, cx, cy, label, f):
    r = 15
    for off, col in ((3, (0, 0, 0)), (0, ACCENT)):
        d.ellipse([cx - r - off / 2, cy - r - off / 2, cx + r + off / 2, cy + r + off / 2],
                  outline=col, width=3 + off)
    tw = d.textlength(label, font=f)
    d.rounded_rectangle([cx + r - 2, cy - r - 20, cx + r + tw + 10, cy - r + 2], 5, fill=ACCENT)
    d.text((cx + r + 4, cy - r - 19), label, font=f, fill=(0, 0, 0))


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--ply", default=str(ROOT / "data/bonsai/point_cloud.ply"))
    ap.add_argument("--cam", default="0")
    ap.add_argument("--width", type=int, default=640, help="tile width")
    ap.add_argument("--out", default=str(ROOT / "docs/roadmap/img/viz-002-all-views-annotated.png"))
    ap.add_argument("--no-callouts", action="store_true")
    args = ap.parse_args()

    tmp = ROOT / "out" / "views"
    tmp.mkdir(parents=True, exist_ok=True)
    subprocess.run([str(EXE), args.ply, "--cam", args.cam, "--out", str(tmp / "tile.png"),
                    "--size", f"{args.width}x{args.width * 9 // 16}", "--view", "all"],
                   check=True, stderr=subprocess.DEVNULL)

    tiles = [Image.open(tmp / f"tile-{v[1]}.png").convert("RGB") for v in VIEWS]
    tw, th = tiles[0].size
    cell_h = HEADER_H + th + CAPTION_H + LEGEND_H
    cols, rows = 2, (len(VIEWS) + 1) // 2
    grid = Image.new("RGB", (cols * tw + (cols + 1) * PAD, rows * cell_h + (rows + 1) * PAD), BG)
    d = ImageDraw.Draw(grid)
    f_head, f_key, f_cap, f_small = font(19, True), font(15, True), font(15), font(13)

    for i, ((key, name, caption, legend, callouts), tile) in enumerate(zip(VIEWS, tiles)):
        x = PAD + (i % cols) * (tw + PAD)
        y = PAD + (i // cols) * (cell_h + PAD)
        # Header: key badge + view name.
        d.rounded_rectangle([x, y + 5, x + 26, y + 29], 5, fill=ACCENT)
        d.text((x + 13, y + 17), key, font=f_key, fill=(0, 0, 0), anchor="mm")
        d.text((x + 36, y + 5), name, font=f_head, fill=FG)
        grid.paste(tile, (x, y + HEADER_H))
        if not args.no_callouts:
            for cx, cy, label in callouts:
                draw_callout(d, x + cx * tw, y + HEADER_H + cy * th, label, f_key)
        cy = y + HEADER_H + th + 6
        d.text((x, cy), caption, font=f_cap, fill=FG)
        if name in CALLOUT_NOTES and not args.no_callouts:
            d.text((x, cy + 22), CALLOUT_NOTES[name], font=f_small, fill=ACCENT)
        if legend:
            draw_legend(d, x, y + HEADER_H + th + CAPTION_H, tw, legend, f_small)

    grid.save(args.out, optimize=True)
    print(f"wrote {args.out} ({grid.size[0]}x{grid.size[1]})")


if __name__ == "__main__":
    main()
