---
status: proposed
date: 2026-11-23
---
# Far blocks stay plain indexed meshes; meshlets are the engine's choice (#365)

The question was whether lodwright should split its far shapes into meshlets, for finer culling and a later mesh-shader
path, or keep writing plain indexed meshes.

## Verdict

**lodwright keeps writing plain indexed meshes.** If the engine wants meshlets, it builds them from the blocks when it
loads them. No change to the generator is needed, and none should be made now.

## Why

- **The output is the game's format.** lodwright writes the block files the game itself reads, and those files have no
  meshlet structure. A meshlet table would be a second, private output that the original game cannot use.
- **A block is already split by cell.** Each block holds up to 16 segments, one per cell, which gives an engine a
  coarse culling unit without any new data.
- **Meshlets are derived data.** meshoptimizer builds them from an index buffer in one pass, with limits that depend on
  the GPU (its README recommends 64 vertices and 126 triangles for NVIDIA, and different limits for earlier AMD GPUs).
  Baking one choice into generated files would fix a per-GPU setting at generation time.
- **The drawing path is not settled.** wgpu 30 exposes mesh shaders as `EXPERIMENTAL_MESH_SHADER`: native only, with its
  shader compiler working on Vulkan alone, and with a documented failure on some Mesa drivers (LLVMPIPE, not RADV).
  Bevy's meshlet renderer, when introduced in 0.14, was described as work in progress, with a fixed vertex shader and
  limits on meshes and materials, and it targets very high-poly scenes. Far blocks are low-poly by design.

## When to look again

If an engine profile shows far-block vertex work or overdraw as a real cost, the first trial is engine-side: build
meshlets at load time from the existing blocks and measure. Only if that load-time cost proves too high is a generated
meshlet cache worth a new ticket, as an optional file beside the blocks.

## Sources

- wgpu `Features` documentation (30.0.2): <https://docs.rs/wgpu/latest/wgpu/struct.Features.html>
- Bevy 0.14 release notes, virtual geometry: <https://bevy.org/news/bevy-0-14/>
- meshoptimizer README (MIT): <https://github.com/zeux/meshoptimizer>
