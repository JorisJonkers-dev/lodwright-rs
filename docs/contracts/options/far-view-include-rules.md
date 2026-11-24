<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Include rules for the far view

`inclusion.include_rules` (empty by default, written only when set) takes objects that nothing
marks visible from far into object LOD:

```json
{ "inclusion": { "include_rules": [ { "models": "lights/street*", "plugin": "Some.esp", "record_type": "STAT" } ] } }
```

- `models` is a pattern on the model path below `meshes`, in any case and with either separator. `*` matches any run of
  characters, as in `exclude_models`.
- `plugin` (optional) keeps only objects whose reference or base object comes from that plugin. A plugin that is not in
  the load order is a request error.
- `record_type` (optional) keeps only base objects of that record type: four capital letters or digits, such as `STAT`.

A reference is taken when its winning base object matches a rule, it is enabled at the start, and it has no enable
parent. A far block cannot follow what a script enables. Matching references that start disabled (persistent ones too)
or have an enable parent are left out and counted in one `include_rule_reference_not_enabled` warning per world.
References that the game's own rule already takes are not added twice, and not counted (an initially disabled
persistent reference marked visible from far is one of them). The exclusion rules (`exclude_plugins`,
`only_plugins`, `exclude_records`, `exclude_models`) apply afterwards, so an exclusion always wins over an include
rule. References are added in load order, which keeps the output deterministic.

An object taken by a rule is drawn with its authored LOD model (`<model>_lod.nif`). Without one it is left out and
reported like any other far model without a LOD model, unless one of two options gives it a simplified copy of its full
model:

```json
{ "inclusion": { "include_rules": [ { "models": "lights/street*", "generate": { "keep": 0.25, "max_error": 0.05 } } ] } }
```

- `generate` (optional, on one rule) gives such a shape to the objects that rule matches, and to no other object. It
  takes the same `keep` and `max_error` as `objects.shapes_from_full_models`, with the same bounds. Another object that
  uses the same model, and that no rule with `generate` matches, stays out and is reported as before. An object matched
  by several rules takes the limits of the first rule, in request order, that has `generate`. The rule also gives shapes
  to objects that a far-view flag already takes, when they match it.
- `objects.shapes_from_full_models` gives such a shape to every object without a LOD model. When both are set, a
  model that a `generate` rule names takes that rule's limits.

An authored LOD model always wins over both. Shapes drawn without lighting (light beams, glows) are left out of such a shape, and a model with no other
solid shape is reported as `far_model_no_solid_shape`. A rule without `generate` keeps the request digest it had before the key
existed.

With no rules, the request digest and every block digest are the same as before the option existed. With rules, the
request digest changes, so the affected blocks are rebuilt.

Source: own design, `docs/decisions/2026-11-23-load-order-rules-for-the-far-view.md` (#385). The rules name nothing
specific to a mod. The caller (a raul Profile, or a hand-written request) owns them.
