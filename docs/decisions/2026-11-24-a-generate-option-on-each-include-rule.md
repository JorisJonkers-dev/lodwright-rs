---
status: accepted
date: 2026-11-24
---
# A `generate` option on each include rule

The include rules (#407) shipped without a switch of their own: an object a rule took was drawn from far only with an
authored LOD model, or with `objects.shapes_from_full_models` on. That switch acts on every object without a LOD model
in the load order. On a 133-plugin load order it also simplified 8 other models, while only the rule's objects were
wanted (objects such as street lamps ship no LOD model).

The owner decided on 2026-11-24 to add `generate` to each include rule, as the record for #385 first proposed. It takes
the same `keep` and `max_error` as the global option. Only the objects a rule with `generate` matches get a shape from
their full model. Another object with the same model that no such rule matches stays out, so a rule never reaches past
what it names. The global option is unchanged, and with it set every object without a LOD model gets a shape as before.

The rule names nothing specific to a mod: the caller (a raul Profile or a hand-written request) owns the rules. A
request without `generate` keeps its digest. The option note is `docs/contracts/options/far-view-include-rules.md`.
