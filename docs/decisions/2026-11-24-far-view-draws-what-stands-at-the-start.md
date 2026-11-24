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
initially disabled, and its enable parent chain does not keep it disabled. A parent with the
"opposite" flag inverts the state of its parent. The winning record of each reference and each parent decides. A parent
that is not a reference of the same world (an actor, or a reference elsewhere) counts as enabled. A chain longer than 16
links or a loop counts as enabled.

The include rules keep their stricter rule: they hold back every reference with an enable parent.

## Amendment, 2026-11-24: persistent disabled references stay (#415)

The first version also left out initially disabled references that are persistent. That dropped landmarks the game's own
far view shows (Black Mountain dish A, the Helios One solar reflectors), so the far view no longer matched the game's
own blocks. Initially disabled persistent references are kept again in the far view's own objects, as
`game-lod-object-rules-disabled-persistent` measured; the owner's complaint (walls disabled through enable parents and
non-persistent disabled references drawn from far) stays fixed. The include rules still hold every initially disabled
reference back.
