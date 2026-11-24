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
that is not a reference of any world of the load order (an actor, or a reference in an interior) counts as enabled. A
chain is followed to its end; a loop counts as enabled.

The cost is that a reference a quest enables later (the solar reflectors) is missing from the far view until the far
view is generated again; the game's own blocks show it before the quest. This follows the owner's rule over the game's
own data. Update 2026-11-24: the include rules and the shapes from shipped blocks follow the same rule; until then they held back
every reference with an enable parent, which kept out the Strip casino lights seen from the Mojave (their parent is a
marker in the Strip world). Parents are now looked up in every world of the load order, not only the child's.
