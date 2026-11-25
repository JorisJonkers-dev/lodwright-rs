<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Far models without the far-view flag

`objects.far_model_without_flag` (off by default, written only when on; tier 3, since 0.18.0) also takes into object
LOD the references that nothing marks visible from far (neither the reference nor its base carries Visible When
Distant) when:

- the base's model has a far model, `<model>_lod.nif`, that a loose file or an archive of the load order holds;
- the base is not a static, activator or tree flagged has-tree-LOD (such a base is drawn as a tree billboard
  already);
- the reference stands when the game starts: not initially disabled (persistent ones included, unlike flagged
  references, whose persistent disabled ones the game's own blocks hold) and not kept disabled by its enable parent
  chain, as for include rules.

Any base type with a model counts (statics, activators, doors, containers and the rest), as for flagged references.

A reference the flag or an include rule already takes is not added a second time. Exclusion rules (`inclusion`) apply
to the added references as to every other. Static collections are not expanded: a collection is taken only when its own
model has a far model.

Mods that ship far models for small, common objects (desert flora: maize, yucca, cactus, sagebrush) often flag only a
few of their bases, so their far models are drawn only with this option. It makes object blocks larger: measure before
turning it on by default.

Setting the option changes the request digest. With it off, the request digest and the output are the same as before
the option existed. Own design, after the behaviour measured on another generator's output (lodwright #426); no source
of that generator was read.
