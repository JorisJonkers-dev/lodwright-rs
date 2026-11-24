<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Far shapes from full models

`objects.shapes_from_full_models` (not set by default, written only when set) draws objects that have no LOD model with
a simplified copy of their full model:

```json
{ "objects": { "shapes_from_full_models": { "keep": 0.25, "max_error": 0.01 } } }
```

`keep` is the share of each shape's triangles to keep, above 0 and at most 1. `max_error` is the largest error allowed,
as a share of the shape's size, from 0 to 1. Simplifying stops at whichever limit it reaches first.

An authored LOD model always wins. A model in which every shape has an alpha property stays out of the far view and is
reported as `far_model_no_solid_shape`, because it may be an effect or glass. A model with at least one shape without
that property keeps all its shapes, because many solid models carry the property on most shapes; a glass shape in such a
model is therefore simplified and drawn too. A corner that two shapes of a model share stays in place, so no crack opens
between them. A static collection whose parts are drawn keeps its parts. The models drawn this way are listed under
`far_model_simplified`.

The error limit usually stops first on real models, so lowering `keep` alone may change little: raise `max_error` too.

Setting the option changes the request digest. Without it, the request digest and the output are the same as before the
option existed. Own design; the simplifier is `meshopt-pure` (see `docs/decisions/2026-11-23-simplifier-for-far-models.md`).
