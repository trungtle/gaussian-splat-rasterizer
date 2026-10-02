# Tooling & Formats — Roadmap

Instrumentation, capture, and extra input formats.

## Decisions

- **Bench before optimizing** (2026-10-01): TOOL-001 gates the GPU area.

## Tasks

<!--task
id: TOOL-001
status: todo
epic: perf
deps: []
-->
### TOOL-001 — Frame timing / `--bench` mode

GPU timestamp queries around the splat pass, CPU timers around sort and
upload, and a `--bench <frames>` mode that orbits the scene headless and
prints mean/p95 per stage.

Verify: `--bench 300` on bonsai prints stable numbers (±5% across runs).

<!--task
id: TOOL-002
status: todo
epic: formats
deps: []
-->
### TOOL-002 — `.splat` / `.spz` loaders

Support the antimatter15 `.splat` (32 B/splat, no SH) and Niantic `.spz`
compressed formats in addition to PLY.

Verify: `bonsai-7k.splat` from the same HF dataset renders like the PLY
(minus view-dependent color).

<!--task
id: TOOL-003
status: done
epic: capture
deps: []
cl: PENDING
-->
### TOOL-003 — Turntable capture

`--turntable <N>` writes N headless frames of a full orbit around the scene
target, starting from `--cam` and offset by `--orbit`, all in one process.
`scripts/make_turntable_gif.py` encodes them into the README GIF with
ffmpeg (palettegen/paletteuse: 320 px, 40 frames, 12 fps, 64 colors,
about 1.1 MB). Walking the dataset camera path is not done yet.

Verify: 40 frames at 320×214 render in 0.52 s (60 at 480×320 in 0.84 s);
[GIF](img/tool-003-turntable.gif).

<!--task
id: TOOL-004
status: todo
epic: ui
deps: []
-->
### TOOL-004 — Viewer overlay

Minimal on-screen UI (egui) for SH degree override, splat scale,
background, and stats.

Verify: toggling SH degree 0 ↔ 3 visibly changes view-dependent highlights.
