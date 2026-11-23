---
status: proposed
date: 2026-11-23
---
# What OpenMW's object paging teaches the far view (#364)

OpenMW builds its distant objects at run time, in pages, from settings the player can change. lodwright writes fixed
blocks ahead of time: one object file per 4 by 4 cells at level 4, plus a high-priority file. This note maps OpenMW's
documented settings onto that design. It uses OpenMW's documentation only, for ideas; no OpenMW code was read or taken.

## Verdict

Take one idea into lodwright: **choose far objects by their apparent size and cost**, as an option that is off by
default and is measured first. The other ideas belong to the engine, or lodwright already has them.

| OpenMW setting (default) | what the documentation says | for lodwright |
| --- | --- | --- |
| `object paging` (true) | batches objects outside the active grid, so distant objects cost relatively little | **Already done.** A block is such a batch, made at generation time |
| `object paging active grid` (true) | pages objects in the active cells too; "significantly improves the framerate" when CPU-limited; merged objects can run into the light limit | **Engine.** lodwright does not write the active cells |
| `object paging merge factor` (250) | higher keeps complex objects paged, at the cost of performance and memory | **Not applicable as it stands.** Every far object is merged into its block |
| `object paging min size` (0.01) | how large an object must appear to be drawn | **Worth a trial**, see below |
| `object paging min size merge factor` (0.3) | lets cheap objects stay visible further than the minimum size allows | **Worth a trial**, the cost half of the same rule |
| `object paging min size cost multiplier` (25) | scales the cost used by the minimum size rule | **Worth a trial**, only if the rule is built |
| `max composite geometry size` (4 cells) | small values give more draw calls and small textures, large values more overdraw | **Fixed by the format.** The game's block layout sets the size; the trade-off explains why not to change it |
| `vertex lod mod`, `LOD factor`, `composite map` settings | terrain detail against speed and load time | **Already covered** by the per-level terrain request |
| groundcover (off) | static, no collision, listed apart from content, merged and rendered in pages to a set distance (6144 units) | **Input to #366**, see below |

## The idea worth trying

Today the plugins' visible-from-far flag alone decides what enters a block. A load order can therefore mark many small
objects, and the rules proposed in the record for #385 can add more. A size-and-cost rule would keep an object when it is
large enough to be seen from the block's nearest far distance, or cheap enough that keeping it costs little.

- It is general: it uses each object's placed bounds and triangle count, with no list of objects.
- It is off by default. The game's own far view stays the standard, and a request without the option stays byte-identical.
- It never removes a named landmark without saying so: `lodwright landmarks` must report no change on the standard list.
- OpenMW's default values do not carry over, because its units and draw distances differ. Thresholds must be measured.

Before building it, measure on a real load order how many triangles fall below a range of thresholds and what the
standard picture pairs lose. If the saving is small, close the idea. No ticket is opened until then.

## For the other tickets

- **#366 (far grass):** OpenMW's separate groundcover kind supports declaring grass apart from objects. The page describes
  merging and paging; it does not say the grass is instanced, so that part of #366 needs its own source.
- **Engine:** paging the active grid is a possible CPU saving for `nv-rs`, with the documented risk to merged objects under
  a light limit. It is an engine question and changes nothing in lodwright.

## Sources

- OpenMW documentation, terrain settings: <https://openmw.readthedocs.io/en/latest/reference/modding/settings/terrain.html>
- OpenMW documentation, groundcover settings:
  <https://openmw.readthedocs.io/en/latest/reference/modding/settings/groundcover.html>
