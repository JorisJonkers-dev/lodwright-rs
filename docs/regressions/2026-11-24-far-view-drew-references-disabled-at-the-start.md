# The far view drew references that are disabled when the game starts

- Issue: #411. Present since object LOD selection was written; noticed in an owner report on a 133-plugin load order.
- Cause: the object step looked only at a reference's own Initially Disabled flag and kept persistent ones; enable
  parents (`XESP`) were not read, so a reference kept disabled by its parent was drawn from far.
- Tests: `objects::tests::a_reference_follows_the_start_state_of_its_enable_parent_chain`,
  `objects::tests::a_later_plugin_that_disables_an_enable_parent_takes_its_children_out` (lodwright-core).
- Correction: leaving out persistent disabled references as well was itself a regression; see
  `2026-11-24-far-view-dropped-persistent-disabled-landmarks.md`.
