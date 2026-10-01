# Completed Tasks

Historical record of finished roadmap work. Each entry keeps its `cl` (commit)
for review. One section per shipped milestone, newest last.

## Milestone 1 — Working rasterizer (CORE-001..007)

**Status 2026-10-01: COMPLETE — bonsai (1.16 M splats, SH 3) and luigi render
correctly headless and in the viewer; 0 validation errors; ~213 fps static on a
desktop discrete GPU.**

**CL: `1cb2aa1`** — CORE-001..006. Review with `git show 1cb2aa1`.

- `src/ply.rs` — PLY parser, activations, 3D covariance, SH reorder
  (`f_rest` is channel-major in the file, `[coeff][rgb]` on the GPU).
- `src/camera.rs` — COLMAP-convention camera, `cameras.json`, orbit.
- `src/renderer.rs` — vulkano device setup (dynamic rendering), staging
  uploads, instanced splat pipeline, CPU counting sort, offscreen readback.
- `src/shaders/splat.vert|frag` — EWA projection, SH eval, eigen quads,
  Gaussian falloff with premultiplied alpha.
- `src/viewer.rs` — winit 0.30 `ApplicationHandler` viewer with swapchain
  recreation and orbit controls.
- `src/main.rs` — CLI, scene framing (robust radius, up axis,
  ray-intersection target), headless path.

**CL: `PENDING`** — CORE-007 (`scripts/fetch_data.py`), README, and this roadmap.
