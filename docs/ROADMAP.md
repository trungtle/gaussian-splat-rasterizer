# Gaussian Splat Rasterizer — Roadmap

This is the roadmap index. Tasks do not live in this file — each is a structured
`<!--task ... -->` block in one of the area docs under `roadmap/`.
Run `python scripts/plans_lint.py` to validate all task headers and print the
next actionable tasks (todo tasks whose deps are all done).
History: [completed.md](roadmap/completed.md).

## Task block schema

```
<!--task
id: AREA-001
status: todo|doing|blocked|done
epic: <slug>
deps: [ID, ID]
cl: <commit hash, required when status: done>
blocked_by: <text, required when status: blocked>
-->
### AREA-001 — Title

Body: what / why / scope.
Verify: how to confirm it's actually done (a command, a measurement, a screenshot).
```

Invariants (enforced by `scripts/plans_lint.py`): ids unique and matching
`^[A-Z]+-[A-Z0-9]+$`; `done` requires a non-empty `cl`; `blocked` requires
`blocked_by`; every `deps` entry must reference an id that exists somewhere
in the roadmap.

## Status rollup

| Area | Doc | Active (todo) | Done | Notes |
|------|-----|----------------|------|-------|
| Core pipeline | [core.md](roadmap/core.md) | 0 | 7 | Milestone 1 complete: bonsai 1.16 M splats renders correctly, ~213 fps static |
| GPU & perf | [gpu.md](roadmap/gpu.md) | 5 | 0 | CPU counting sort is the bottleneck while the camera moves; gated on TOOL-001 + QUAL-001 |
| Quality | [quality.md](roadmap/quality.md) | 4 | 0 | No numeric ground truth yet — QUAL-001 golden test is the safety net for GPU work |
| Tooling & formats | [tools.md](roadmap/tools.md) | 4 | 0 | TOOL-001 bench mode gates the GPU area |
| Shaders & viz | [viz.md](roadmap/viz.md) | 3 | 2 | Shaders now Slang (`slangc` in build.rs); 10 debug views — guide in [debug-views.md](debug-views.md) |

![bonsai, dataset camera 0](roadmap/img/core-005-bonsai-cam0.png)

## Next actionable

1. **QUAL-001 — golden-image test**, then **TOOL-001 — `--bench` mode.**
   The GPU rewrite (GPU-001..003) changes every stage of the pipeline; it
   needs a pixel regression check and before/after frame times to be
   reviewable. Both are small.
2. Then **GPU-001 → GPU-002** (compute preprocess + GPU radix sort), which
   removes the CPU sort bottleneck during camera motion.

## Recent merges

| CL | What | Tasks |
|----|------|-------|
| `27f8c9a` | Rasterizer: PLY loader, vulkano splat pipeline, headless + viewer | CORE-001..006 |
| `ad4f5a3` | Data fetch script, README, roadmap docs | CORE-007 |
| `fd4b26c` | GLSL → Slang port + debug/visualization views | VIZ-001, VIZ-002 |
| `7e78d3b`, `40cce82` | Annotated view grid; key 1 resets to normal render | VIZ-002 follow-ups |

Per-commit review: `git show <CL>`. For a merge commit `<merge>`, the branch's
full diff is `git diff <merge>^1 <merge>^2` and its commits are
`git log <merge>^1..<merge>^2 --oneline`.
