---
status: proposed
date: 2026-11-23
---
# Load-order rules that opt objects into the far view (#385)

Some mods place objects that have no far model and that nothing marks visible from far, so they vanish at a distance.
This record proposes one general mechanism for that. It contains no object-specific code or list.

## Test case

Dual Wasteland Street Lights for Tale of Two Wastelands is the measuring case only. On a 133-plugin load order its plugin
stores 8,056 references in worldspaces: 7,133 new and 923 overrides. It ships no far models and marks nothing visible
from far. 5,950 of the references are light beams and glows, which do not belong in a far block. Of the 2,106 solid
objects, 248 start disabled and 387 have an enable parent. Counted per model over the winning records in all worlds,
571 enabled solid references use a model that has a far model, and 1,282 (435,893 triangles as full models) use one
that has none.

## Decision

1. **Where rules live:** in the request, as `inclusion.include_rules`, next to the existing exclusion rules. The caller
   (a profile in `raul`, or a hand-written request) owns them. No companion plugin is needed to generate, and no plugin
   is changed.
2. **Matching:** each rule names a model pattern (the existing `*` glob on the path below `meshes`), and optionally a
   plugin and a record type. Exclusion rules still win over inclusion rules.
3. **Which model:** the authored `_lod` model when one exists. Without one the object stays out and is reported, unless
   the rule asks for `generate = true`, which uses a simplified full model (see the cost below). Effects are never
   simplified: a rule that matches a model with no solid shape is reported.
4. **Disabled objects:** a far block cannot follow a script, so a reference taken through a rule must be enabled at
   the start **and** have no enable parent. Persistent disabled references get no exception here. Each reference left
   out this way is counted in the warnings.
5. **Determinism:** rules are applied in request order to references in load order, and generated shapes depend only on
   their input bytes. The field is left out of the serialised request when empty, so a load order with no rules keeps
   the same request digest and byte-identical output.

6. **Related option:** `objects.shapes_from_shipped_blocks` (#338) already follows these rules for objects whose far
   shape exists only in a game's shipped far blocks: the caller names world and models, and lodwright holds no list.

## Cost

| part | size | note |
| --- | --- | --- |
| rules in the request, matching, warnings, tests | small, 2 to 3 days | uses the existing filter and block writer |
| rules with authored far models only | included above | covers the 571 references of the test case |
| simplifier for models without a far model | large, 1 to 2 weeks | lodwright has none today: `tin.rs` is for terrain only (#368, #369) |
| own redistributable add-on | a further 3 to 5 days | needs a writer for plugin overrides; today only the stub plugin exists |

The durations are estimates, not measurements. The 435,893 triangles are spread over all worlds; for scale, the generated
level 4 object blocks of the Capital Wasteland alone hold about 1.66 million. Drawing those models unsimplified would be
a large addition for thin objects, so the target is roughly a tenth of the triangles. Whether lamp posts keep a usable
silhouette at that rate is unmeasured.

**Building our own add-on is possible but is the last step.** Generating far blocks locally from rules needs no
redistribution at all and is the recommended path. Publishing an add-on would need the mod author's permission.

## Open before any redistribution

- **Permissions are not verified.** The mod's page (Nexus Mods, New Vegas, mod 76230) refused automated reading, and its
  local metadata holds no permission text. An owner must read its permissions tab.
- **No existing LOD add-on was found** in public web searches for this mod, but the same site's pages could not be
  opened, so this is not proof that none exists.

## Synthetic fixture plan

Built in code with `lodwright-testkit`, with no game data:

- a load order with no rules: output and request digest byte-identical to a run without the field;
- a rule on a model with an authored far model draws it; a model without one is left out and reported;
- a reference initially disabled, one persistent and disabled, and one with an enable parent all stay out;
- an exclusion rule beats an inclusion rule, and two runs give identical bytes;
- `generate = true` (with the simplifier): fewer triangles under a stated error, and a model with no solid shape is
  reported.
