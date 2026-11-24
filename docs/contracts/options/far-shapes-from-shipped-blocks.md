<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Far shapes from shipped blocks

`objects.shapes_from_shipped_blocks` (empty by default, written only when set) lists, per world, the models whose far
shape is taken from the far blocks the games ship:

```json
{ "objects": { "shapes_from_shipped_blocks": [ { "world": "SomeWorld", "models": ["architecture/some/model.nif"] } ] } }
```

A world is named by its editor id and a model by its path below `meshes`, in any case and with either separator. A
world the load order does not have is ignored. Only references that stand in the world when the game starts are taken:
not initially disabled, and not kept disabled by their enable parent chain (a parent in another world counts too). The inclusion rules apply as usual. A piece of a shipped block belongs to a reference
when it lies inside the reference's placed object bounds; a reference with no block or no shape is left out and
reported in the warnings. Shapes are read from the user's own game archives at generation time. With no rules, the
output and the request digest are the same as before the option existed.
