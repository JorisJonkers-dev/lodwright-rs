<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Terrain faces under the water

`terrain.drop_under_water` (a number of world units, 0 to 100000; absent by default, written only when set; tier 3,
since 0.9.0) leaves out terrain faces that the water hides: a face is left out when its three corners all lie at least
that many units below the water plane of the cell its centre is in (cells without water keep all their faces). Vertices
no kept face uses are left out too. If that would leave a block no face, the block keeps all of them. Own design
(`docs/spec/terrain-lod.md`, section 7). Skirts, when on, are added after the drop. Absent, output and digests are as
before; set, the offset enters the digest of every terrain block. The result's vertex and triangle counts of a block
are the ones written.
