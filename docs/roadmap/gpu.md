# GPU Pipeline & Performance — Roadmap

Everything per-splat except the final blend currently happens either on the
CPU (depth sort) or redundantly per vertex (projection + SH run 6× per
splat). This area moves the pipeline to the classic GPU layout:
compute preprocess → GPU sort → indirect draw, and optionally a tile-based
compute rasterizer like the reference CUDA implementation.

## Decisions

- **Measure first** (2026-10-01): TOOL-001 (timing) lands before any of these,
  so every task here reports a before/after frame time.
- **Pending — sort algorithm**: candidates are (a) 4/8-bit LSD radix sort in
  compute (portable, well understood), (b) Onesweep (faster, but relies on
  forward-progress guarantees Vulkan does not promise), (c) bitonic (simple,
  O(n log² n), too slow beyond ~1 M). Leaning (a); decide in GPU-002.
- **Pending — raster backend**: keep the quad/blend pipeline as default and
  add the tile rasterizer (GPU-005) as an alternative, or replace it. Decide
  after GPU-003 numbers.

## Tasks

<!--task
id: GPU-001
status: todo
epic: gpu-pipeline
deps: [TOOL-001, QUAL-001]
-->
### GPU-001 — Compute preprocess pass

One compute thread per splat: frustum cull, 2D covariance + eigen
decomposition, SH → RGB, write a compact `ProjectedSplat` (center, axes,
color, opacity) and a depth key. The vertex shader becomes a trivial fetch.

Verify: QUAL-001 golden test passes (max abs diff ≤ 1/255 vs. CORE-005);
splat-pass GPU time drops (TOOL-001 numbers).

<!--task
id: GPU-002
status: todo
epic: gpu-pipeline
deps: [GPU-001]
-->
### GPU-002 — GPU radix sort of depth keys

Replace the CPU counting sort with a compute radix sort (32-bit keys from
the preprocess pass, payload = splat index). Removes the per-frame 4.6 MB
index upload.

Verify: same image as the CPU sort; sort < 2 ms for 1.16 M keys; viewer fps
while orbiting no longer CPU-bound (`--bench`).

<!--task
id: GPU-003
status: todo
epic: gpu-pipeline
deps: [GPU-002]
-->
### GPU-003 — Indirect draw of visible splats only

Preprocess appends visible splats with an atomic counter; draw via
`draw_indirect` so culled splats cost nothing.

Verify: indirect instance count < total for interior views; frame time
improves vs. GPU-002.

<!--task
id: GPU-004
status: todo
epic: memory
deps: []
-->
### GPU-004 — Half-precision SH storage

SH buffer is 222 MB for bonsai in f32. Store as f16 (or f16 DC + quantized
rest) and unpack in the shader.

Verify: SH buffer size halves; PSNR vs. f32 render > 45 dB.

<!--task
id: GPU-005
status: todo
epic: tile-raster
deps: [GPU-002]
-->
### GPU-005 — Tile-based compute rasterizer

Reference-style backend: duplicate splats per overlapped 16×16 tile with
(tile, depth) keys, sort, compute per-tile ranges, then one workgroup per
tile blends front-to-back with early termination at T < 1e-4. Selectable via
`--backend tile`.

Verify: PSNR vs. quad backend > 40 dB on bonsai; frame times compared in a
report under `roadmap/reports/`.
