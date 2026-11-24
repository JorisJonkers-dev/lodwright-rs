---
status: accepted
date: 2026-11-24
---
# The far view draws what stands in the world when the game starts

The owner asked on 2026-11-24 that the far view respect what is enabled and disabled: only what is actually visible is
drawn. Until then lodwright kept initially disabled references that are persistent, because the game's own blocks hold
them (`game-lod-object-rules-disabled-persistent`: seven solar reflectors in the unmodded game), and it ignored enable
parents.

From now on a reference enters the far view only when it stands in the world at the start of a game: it is not
initially disabled, and its enable parent chain does not keep it disabled. The one exception is an initially disabled
reference that is also persistent: the game's own blocks hold those, so they stay (see the amendment below), whatever
their enable parent. A parent with the "opposite" flag inverts the state of its parent. The winning record of each
reference and each parent decides. A parent that is not a reference of any world of the load order (an actor, or a
reference in an interior) counts as enabled. A chain is followed to its end (#413); a loop counts as enabled.

Update 2026-11-24: the include rules and the shapes from shipped blocks follow the same start-state rule; until then they
held back every reference with an enable parent, which kept out the Strip casino lights seen from the Mojave (their
parent is a marker in the Strip world). Parents are now looked up in every world of the load order, not only the
child's. The include rules still hold every initially disabled reference back, persistent ones too.

## Amendment, 2026-11-24: persistent disabled references stay (#415)

The first version also left out initially disabled references that are persistent. That dropped landmarks the game's own
far view shows (Black Mountain dish A, the Helios One solar reflectors), so the far view no longer matched the game's
own blocks. Initially disabled persistent references are kept again in the far view's own objects, as
`game-lod-object-rules-disabled-persistent` measured; the owner's complaint (walls disabled through enable parents and
non-persistent disabled references drawn from far) stays fixed.
