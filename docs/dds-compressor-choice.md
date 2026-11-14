# DDS compressor choice

`lodwright-dds` encodes BC1, BC3 and BC5 with `texpresso` 2.0.2 (MIT, pure Rust) and writes the container with
`ddsfile` 0.6.0 (MIT). The other candidate named in the plan, `intel_tex_2` 0.5.0 (MIT or Apache-2.0), was measured
by hand and not chosen.

## Numbers

- Input: synthetic 2048 by 2048 RGBA8, a gradient and uniform noise (the generators of `lodwright-testkit`).
- Machine: AMD Ryzen 7 5800X3D, 16 threads, Linux 7.3, rustc 1.99.1, release profile, one thread
  (`texpresso` without its optional `rayon` feature; `intel_tex_2` ran single-threaded as well).
- Command: a scratch crate outside the repository with both crates as dependencies, built with
  `RUSTFLAGS="-C link-arg=-lstdc++" cargo run --release` (see below for why the flag). The harness encoded each
  image once per format, decoded the result with `texpresso`'s decoder and printed the time and the PSNR.
  `texpresso` ran with its default parameters (cluster fit, perceptual weights).

| Input | Format | `texpresso` | `intel_tex_2` |
| --- | --- | --- | --- |
| gradient | BC1 | 213 ms, 44.4 dB | 9 ms, 41.9 dB |
| gradient | BC3 | 210 ms, 45.7 dB | 11 ms, 43.2 dB |
| gradient | BC5 | 97 ms, exact | 28 ms, exact |
| noise | BC3 | 6708 ms, 14.7 dB | 11 ms, 14.8 dB |
| noise | BC5 | 94 ms, 29.2 dB | 9 ms, 29.3 dB |

(The noise BC1 rows differ only because noise carries random alpha, which `texpresso` read as 1-bit transparency;
`lodwright-dds` now encodes BC1 as opaque colour, after which it measures 13.4 dB on noise, like `intel_tex_2`.)

`cargo bench -p lodwright-dds --bench encode` runs the same kind of measurement against the shipped crate
(`Quality::Fast` is range fit, `Quality::Normal` is cluster fit).

## Why `texpresso`

- `intel_tex_2` is 20 to 600 times faster, but it links pre-built native static libraries that ship inside the crate
  (ISPC kernels and a C++ ASTC encoder). On this machine the link failed with an undefined `__gxx_personality_v0`
  until `-lstdc++` was added by hand, and those libraries exist only for a fixed list of targets. Building the kernels
  from source needs the ISPC compiler.
- `texpresso` is plain Rust, builds everywhere the project builds, has no binary blobs to audit, and brings the
  decoder the tests need.
- A terrain block texture is at most 2048 by 2048 and blocks are independent, so a caller can encode several at once;
  the single-threaded cost of a smooth 2048 by 2048 BC3 is 0.2 s at `Quality::Normal` and 0.1 s at `Quality::Fast`. Worst-case
  noise costs 7 s at `Quality::Normal` (0.17 s at `Quality::Fast`), so the real cost has to be checked on real textures
  on the owner's machine, which this repository never holds.

If texture compression time becomes the bottleneck of `generate`, revisit this with real block counts from the owner's
machine; the encoder sits behind `lodwright_dds::encode`, so the swap stays inside the crate.
