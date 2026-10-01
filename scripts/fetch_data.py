#!/usr/bin/env python3
"""Download sample 3D Gaussian Splatting scenes into data/.

Source: Hugging Face dataset dylanebert/3dgs (pretrained INRIA 3DGS outputs).
  luigi   ~1 MB    SH degree 0, small object (quick smoke test)
  bonsai  ~287 MB  SH degree 3, Mip-NeRF 360 scene, 7k iterations, + cameras.json

usage: python scripts/fetch_data.py [luigi] [bonsai]   (default: both)
"""
import sys
import urllib.request
from pathlib import Path

BASE = "https://huggingface.co/datasets/dylanebert/3dgs/resolve/main"
ROOT = Path(__file__).resolve().parent.parent / "data"
SCENES = {
    "luigi": [("luigi/luigi.ply", "luigi.ply")],
    "bonsai": [
        ("bonsai/cameras.json", "bonsai/cameras.json"),
        ("bonsai/point_cloud/iteration_7000/point_cloud.ply", "bonsai/point_cloud.ply"),
    ],
}


def fetch(src, dst):
    dst = ROOT / dst
    if dst.exists():
        print(f"exists: {dst}")
        return
    dst.parent.mkdir(parents=True, exist_ok=True)
    tmp = dst.with_suffix(dst.suffix + ".part")
    print(f"downloading {src} ...")
    urllib.request.urlretrieve(f"{BASE}/{src}", tmp)
    tmp.rename(dst)


for name in sys.argv[1:] or SCENES:
    for src, dst in SCENES[name]:
        fetch(src, dst)
