# Completed Tasks

Historical record of finished roadmap work. Each entry keeps its `cl` (commit)
for review. One section per shipped milestone, newest last.

## Milestone 1 — Working rasterizer (CORE-001..007)

**Status 2026-10-01: COMPLETE — bonsai (1.16 M splats, SH 3) and a small SH-0 test scene render
correctly headless and in the viewer; 0 validation errors; ~213 fps static on a
desktop discrete GPU.**

**CL: `27f8c9a`** — CORE-001..006. Review with `git show 27f8c9a`.

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

**CL: `ad4f5a3`** — CORE-007 (`scripts/fetch_data.py`), README, and this roadmap.

## Slang shaders + debug views (VIZ-001..002)

**Status 2026-10-01: COMPLETE — the Slang port matches the GLSL render (max diff
1/255, mean 1e-4); 10 debug views render headless and in the viewer.**

**CL: `fd4b26c`** — VIZ-001, VIZ-002.

- `src/shaders/splat.slang` replaces `splat.vert` / `splat.frag`: same math,
  plus the view-mode switch, Turbo colormap, and splat-scale / min-opacity /
  SH-cap knobs.
- `build.rs` runs `slangc` per entry point to `$OUT_DIR/*.spv`;
  `src/renderer.rs` loads the SPIR-V with `ShaderModule::new`, has a
  hand-written `Push` struct and a second additive pipeline for overdraw, and
  tracks the robust depth range during the sort.
- `src/debug.rs` adds `ViewMode` and `DebugSettings`. `src/main.rs` adds
  `--view`, `--splat-scale`, `--min-opacity`, `--sh`. `src/viewer.rs` adds the
  key bindings (help printed at startup).
- Gotcha found: `SV_InstanceID` needs DrawParameters; use `SV_VulkanInstanceID`.
