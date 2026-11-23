---
status: proposed
date: 2026-11-23
---
# Which simplifier for far models that have no LOD model (#368)

If a far shape is ever generated from a full model (#369, and the `generate` option in the record for #385), lodwright
needs a mesh simplifier. Its only one today, `tin.rs`, is for terrain height fields. This note picks the candidate and
records a spike on synthetic meshes. **No dependency is added here.**

## Verdict

Use **`meshopt-pure` 1.3.0** (MIT), pinned, when #369 is built. Keep it behind that off-by-default option, and check the
points under "Before adopting it" first.

| candidate | licence | fits the workspace | verdict |
| --- | --- | --- | --- |
| `meshopt-pure` 1.3.0 | MIT | Rust only; no C++ toolchain | **chosen** |
| `meshopt` 0.7.0 | MIT OR Apache-2.0 | binds the C++ library, adding a C++ build on three systems | the mature fallback |
| `meshoptimizer-rs` 0.2.20 | MIT | Rust, but its latest release is two days old: fails the 7-day crate age rule today | not considered further now |
| an own quadric simplifier | GPL-3.0-only | no dependency | the most work, and the same algorithm with less testing |

`meshopt-pure` is an unofficial port of meshoptimizer, with few downloads so far. That is its main risk. Because its API
follows meshoptimizer's, moving to the `meshopt` binding later would be a small change.

## Spike

Three synthetic meshes, simplified with `simplify` on one thread. "Measured" is the largest distance from an original
vertex to the simplified surface, as a share of the mesh's diagonal. It is one-directional, so it can miss a surface that
moved away from every original vertex.

| mesh | triangles | target | error limit | result | measured error | box kept |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| thin post with an arm | 724 | 25% | 1% | 180 | 0.00% | 100% |
| thin post with an arm | 724 | 10% | 1% | 72 | 0.12% | 100% |
| wall with window holes | 3,520 | 25% | 1% | 880 | 0.03% | 100% |
| wall with window holes | 3,520 | 10% | 1% | 352 | 0.08% | 100% |
| wall, borders locked | 3,520 | 10% | 5% | 686 | 0.20% | 100% |
| bumpy rock | 3,968 | 25% | 1% | 1,092 | 1.15% | 100% |
| bumpy rock | 3,968 | 10% | 1% | 1,065 | 1.15% | 100% |
| bumpy rock | 3,968 | 10% | 5% | 395 | 1.56% | 99% |
| bumpy rock, borders locked | 3,968 | 10% | 5% | 396 | 2.92% | 98% |

- Each mesh took under a millisecond in a release build.
- Thin posts and flat walls reach a tenth of their triangles with almost no error. That is the case the record for #385
  needs. Rough organic shapes stop at the error limit, which is the behaviour wanted.
- Locking borders roughly doubles the triangles left on an open mesh such as the wall, so it should be used only where a
  border must stay.
- Two runs gave identical indices, and so did the builds with and without the crate's SIMD feature, on one x86-64 system.

## Before adopting it

1. **Output across systems.** lodwright's output must not depend on the platform. The spike ran on x86-64 Linux only;
   the indices must be compared on Windows and macOS (ARM), with SIMD off if they differ.
2. **Attributes.** The spike measured positions only. UV seams and normals are the work of #369
   (`simplify_with_attributes`).
3. **Real models.** Synthetic shapes do not show how a real full model's textures and silhouette hold up.
4. **Dependency checks.** `cargo deny` and the crate age rule apply when it is added. The crate denies unsafe code at
   its root; the exceptions are in its SIMD modules, behind a default feature that can be turned off.

## Sources

- meshoptimizer (MIT): <https://github.com/zeux/meshoptimizer>
- `meshopt-pure`: <https://crates.io/crates/meshopt-pure>
- `meshopt`: <https://crates.io/crates/meshopt>
- `meshoptimizer-rs`: <https://crates.io/crates/meshoptimizer-rs>
