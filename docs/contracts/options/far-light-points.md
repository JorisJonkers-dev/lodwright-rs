<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Far light points

`objects.far_lights` (off by default, written only when on; tier 3, since 0.18.0) draws the steady lights of a world as
small light points in the far view, so street lamps and lit buildings show from a distance at night.

Which lights: exterior light references (`LIGH`) that stand when the game starts (not initially disabled, not kept
disabled by their enable parent chain), whose light reaches at least 256 game units and is not negative, flickering,
pulsing or off by default (`fnv-light-data`), and that stand within 300 units (on the ground plane) of an object drawn in
the same object block. A light alone in the open gets no point.

What is written: in the normal file of each object block that has such lights, one more shape beside the object shapes:
a plain `NiTriShape` with three crossed squares 192 units wide per light (one flat, two upright, each written with both
windings so it shows from either side), vertex colours at the light's full colour, a `BSShaderNoLightingProperty`
(shader type 33, z test on, z write off, vertex colours on, falloff unset) naming the world's light point texture
`textures\landscape\lod\<world>\blocks\<world>.farlights.dds` (lower case, no `Data\` prefix, as the game's own glow
effects name theirs; a soft round spot, written once per world when any point is), an additive `NiAlphaProperty` (flags
`0x000D`: source SRC_ALPHA, destination ONE, no alpha test, so the engine can fade the points through their alpha) and
a `NiStringExtraData` named `FarLights` on the shape. The high files get no points. Readers of object
blocks (`verify`, `landmarks`) pass the light shape over.

The `FarLights` marker is for the engine: it is meant to show the shape only at night, from the climate's sunrise and
sunset times. An engine that does not know the marker draws the points by day too, so the option stays off until the
engine honours it.

Setting the option changes the request digest. With it off, the request digest and the output are the same as before
the option existed. Own design (sizes, reach, dimming, texture); the block layouts are `nif-no-lighting-property-layout`,
`nif-alpha-flags` and `nif-string-extra-data-layout`.
