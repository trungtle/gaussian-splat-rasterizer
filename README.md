# gaussian-splat-rasterizer

A 3D Gaussian Splatting renderer in Rust on [vulkano](https://github.com/vulkano-rs/vulkano).
Loads standard 3DGS `.ply` files (SH degree 0–3), renders them in an
interactive viewer or headless to PNG.

![bonsai](docs/roadmap/img/core-005-bonsai-cam0.png)

## Requirements

- Rust (2024 edition), a Vulkan 1.3 driver (or `VK_KHR_dynamic_rendering`)
- Vulkan SDK (shaders are compiled at build time via shaderc)

## Quick start

```bash
python scripts/fetch_data.py
cargo run --release -- data/bonsai/point_cloud.ply
```

Headless:

```bash
cargo run --release -- data/bonsai/point_cloud.ply --out out/bonsai.png --size 1280x720
```

Viewer controls: left-drag orbit · right-drag pan · wheel zoom ·
N / P next / previous dataset camera · R reset · Esc quit.
Run with `--help` for all options.

## How it works

1. `src/ply.rs` — parse the PLY, apply activations, precompute 3D covariances.
2. `src/renderer.rs` — CPU counting sort of splats by view depth (back to front).
3. `src/shaders/splat.vert` — per instance: project the covariance (EWA),
   evaluate spherical harmonics, emit a quad aligned with the 2D eigenvectors.
4. `src/shaders/splat.frag` — Gaussian falloff, premultiplied alpha blending.

Planned work (GPU sort, compute preprocess, tile rasterizer, tests) is tracked in
[docs/ROADMAP.md](docs/ROADMAP.md).
