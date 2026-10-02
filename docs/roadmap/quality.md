# Image Quality & Correctness — Roadmap

Current output is visually right but has no numeric ground truth. These
tasks add regression protection first, then quality features.

## Decisions

- **Golden images live in the repo** (2026-10-01, small, ≤ 640 px) so
  `cargo test` can diff without the 287 MB dataset; tests needing bonsai skip
  if `data/` is absent.

## Tasks

<!--task
id: QUAL-001
status: todo
epic: testing
deps: []
-->
### QUAL-001 — Golden-image regression test

`cargo test` renders a small procedurally generated Gaussian scene (no
download needed, and bonsai cam 0 if present) headless and compares to
committed goldens with a PSNR / max-diff threshold.

Verify: test passes; deliberately flipping an SH sign makes it fail.

<!--task
id: QUAL-002
status: todo
epic: parity
deps: [QUAL-001]
-->
### QUAL-002 — Parity against the reference renderer

Render bonsai views with the reference `diff-gaussian-rasterization` (or
gsplat) and compare. Needs a Python/CUDA environment — record the exact
commands used.

Verify: mean PSNR vs. reference > 40 dB over ≥ 5 cameras.

<!--task
id: QUAL-003
status: todo
epic: aa
deps: [QUAL-001]
-->
### QUAL-003 — Mip-Splatting style 2D filter

Replace the fixed +0.3 dilation with an opacity-compensated filter (scale
opacity by sqrt(det Σ / det(Σ + sI))) to reduce zoom-out aliasing and
dilation artifacts. Toggle via flag.

Verify: before/after pair at 4× zoom-out; flag off = bit-identical to today.

<!--task
id: QUAL-004
status: todo
epic: sorting
deps: [GPU-001, TOOL-003]
-->
### QUAL-004 — Popping reduction

A global per-splat depth sort pops when orbiting. Evaluate StopThePop-style
hierarchical per-tile resorting (natural fit for the GPU-005 tile backend).

Verify: turntable capture (TOOL-003) shows reduced flicker, quantified by
frame-to-frame diff on a slow orbit.
