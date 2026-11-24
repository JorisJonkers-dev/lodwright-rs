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
References that the game's own rule already takes are not added twice. The exclusion rules (`exclude_plugins`,
`only_plugins`, `exclude_records`, `exclude_models`) apply afterwards, so an exclusion always wins over an include
rule. References are added in load order, which keeps the output deterministic.

An object taken by a rule is drawn with its authored LOD model (`<model>_lod.nif`). Without one it is left out and
reported like any other far model without a LOD model, unless `objects.shapes_from_full_models` is set, in which case it
is drawn as a simplified copy of its full model. There is no per-rule switch for this. Effects whose shapes all have an
alpha property are never simplified and are reported.

With no rules, the request digest and every block digest are the same as before the option existed. With rules, the
request digest changes, so the affected blocks are rebuilt.

Source: own design, `docs/decisions/2026-11-23-load-order-rules-for-the-far-view.md` (#385). The rules name nothing
specific to a mod. The caller (a raul Profile, or a hand-written request) owns them.
