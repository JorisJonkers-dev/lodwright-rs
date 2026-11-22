<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Shared corners of the water planes

`terrain.water_shared_vertices` (a switch, off by default, written only when on; tier 3, since 0.14.0) writes a corner
that two rectangles of a water plane have in common once, at levels 8 to 32 (level 4 keeps one rectangle per
cell segment). The triangles are the same corner for corner, so the water overlap and the covered ground do not change;
only the number of vertices in the file falls. Off, output and digests are as before; on, the switch enters the digest
of every terrain block that has water.
