#!/usr/bin/env python3
"""Render a turntable orbit headless and encode a small looping GIF for the README.

Defaults keep the GIF around 1 MB: 320 px wide, 40 frames at 12 fps, 64-color
palette. Needs a release build and ffmpeg on PATH.

usage: python scripts/make_turntable_gif.py [--ply PLY] [--cam N] [--frames N]
                                            [--width W] [--fps F] [--colors C] [--out GIF]
"""
import argparse
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXE = ROOT / "target" / "release" / ("gaussian-splat-rasterizer.exe" if sys.platform == "win32" else "gaussian-splat-rasterizer")


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--ply", default=str(ROOT / "data/bonsai/point_cloud.ply"))
    ap.add_argument("--cam", default="0")
    ap.add_argument("--frames", type=int, default=40)
    ap.add_argument("--width", type=int, default=320)
    ap.add_argument("--fps", type=int, default=12)
    ap.add_argument("--colors", type=int, default=64)
    ap.add_argument("--out", default=str(ROOT / "docs/roadmap/img/tool-003-turntable.gif"))
    args = ap.parse_args()
    if not shutil.which("ffmpeg"):
        sys.exit("ffmpeg not found on PATH")

    tmp = ROOT / "out" / "turntable"
    if tmp.exists():
        for f in tmp.glob("f-*.png"):
            f.unlink()
    tmp.mkdir(parents=True, exist_ok=True)
    height = round(args.width * 2 / 3) // 2 * 2
    subprocess.run([str(EXE), args.ply, "--cam", args.cam, "--out", str(tmp / "f.png"),
                    "--size", f"{args.width}x{height}", "--turntable", str(args.frames)], check=True)
    palette = (f"split[a][b];[a]palettegen=max_colors={args.colors}:stats_mode=full[p];"
               "[b][p]paletteuse=dither=bayer:bayer_scale=5")
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-framerate", str(args.fps),
                    "-i", str(tmp / "f-%03d.png"), "-vf", palette, "-loop", "0", args.out], check=True)
    print(f"wrote {args.out} ({Path(args.out).stat().st_size / 1e6:.2f} MB)")


if __name__ == "__main__":
    main()
