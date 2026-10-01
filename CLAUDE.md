# gaussian-splat-rasterizer

Rust + vulkano 0.35 3D Gaussian Splatting renderer. See README.md for usage.

## Roadmap / task tracking

- Task tracker: `docs/ROADMAP.md` (rollup table + next actionable), area docs in
  `docs/roadmap/*.md`. Status check: `python scripts/plans_lint.py`.
- When a task's status changes, update the area doc **and** the rollup table in
  the same change so they never drift. Done tasks need `cl:` = commit hash.
- Visual tasks: capture with the headless path
  (`target/release/gaussian-splat-rasterizer <ply> --out <png> [--cam N] [--orbit deg]`)
  into `docs/roadmap/img/<task-id>-<slug>.png`; before/after pairs for visible changes.
- Feature branches merge with `git merge --no-ff`; review a merge with
  `git diff <merge>^1 <merge>^2` and `git log <merge>^1..<merge>^2 --oneline`.

## Conventions

- Shaders are Slang (`src/shaders/splat.slang`), built by `build.rs` via `slangc`.
  Use `SV_VulkanVertexID`/`SV_VulkanInstanceID`; `Push` in `src/renderer.rs` mirrors
  the Slang struct by hand. View-mode order = `ViewMode::ALL` in `src/debug.rs`.
  See `docs/roadmap/viz.md` Decisions.

- Camera space is COLMAP: +x right, +y down, +z forward (== Vulkan clip space).
- Render targets are UNORM, never SRGB (3DGS blends in display space).
- Sample data lives in `data/` (gitignored); fetch with `python scripts/fetch_data.py`.
- Validation check: set `VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation`, redirect
  output to a file, and grep for `VUID` rather than reading the log.
