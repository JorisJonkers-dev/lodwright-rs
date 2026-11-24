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
initially disabled, persistent or not, and its enable parent chain does not keep it disabled. A parent with the
"opposite" flag inverts the state of its parent. The winning record of each reference and each parent decides. A parent
that is not a reference of the same world (an actor, or a reference elsewhere) counts as enabled. A chain longer than 16
links or a loop counts as enabled.

The cost is that a reference a quest enables later (the solar reflectors) is missing from the far view until the far
view is generated again; the game's own blocks show it before the quest. This follows the owner's rule over the game's
own data. The include rules keep their stricter rule: they hold back every reference with an enable parent.
