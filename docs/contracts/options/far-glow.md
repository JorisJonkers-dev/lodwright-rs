<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Far glow

`objects.far_glow` (off by default, written only when on) lets lit windows, signs and lamps glow in the far view at
night:

```json
{ "objects": { "far_glow": true } }
```

For each object atlas page with at least one texture that has a glow map (slot 2 of the shape's texture set, the `_g`
maps of lit buildings and signs), lodwright writes a glow atlas beside the page:
`textures/landscape/lod/<world>/blocks/<world>.buildings_g.dds` (`buildings01_g.dds` and so on for later pages). It
has the layout of the page at half its size, as the normal atlas has, holds each texture's glow map in that texture's
rectangle and is black everywhere else. A glow map that cannot be read stays black. A page whose textures have no
readable glow map gets no glow atlas.

The shapes of that page name the glow atlas in the glow slot of their texture set and get the block's one
`NiMaterialProperty`, with emissive colour (1, 1, 1). The engine scales a far block's glow map by the material's emissive
colour. Without a material, as the blocks are written otherwise, a glow map draws nothing, and where the glow atlas is
black nothing is added. The game's own far blocks have no glow map and no material, so a block with far glow has one
block type more than the game's (`lodwright verify --reference` reports "block types differ" for it).

Far shapes from full models (`objects.shapes_from_full_models`, or an include rule's `generate`) keep the glow map of the
full model, so a building that has no LOD model can glow from its own `_g` maps.

A shape drawn without lighting (`BSShaderNoLightingProperty`) is an effect and is otherwise left out of far shapes made
from full models. With far glow on, such a shape that names a texture is a glow card: what it shows is light it gives
off, so its far shape is black in the atlas and its texture, each colour weighted by its alpha (the shape is blended
by it), is its glow map. A model of glow cards alone gets a far shape too. The lights of the Strip casinos as seen from
the Mojave are such models (separate light-strip models in front of each casino, not the casino models themselves).
A map named `_g` that sits in slot 5 of a texture set (the environment mask) is not a glow map and is not read as one.

Turning the option on changes the request digest. Off, the request digest and every written file are the same as before
the option existed. Off by default until night pictures show that it helps and that nothing gets worse by day.
