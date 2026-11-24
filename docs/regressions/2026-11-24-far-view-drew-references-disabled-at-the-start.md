# The far view drew references that are disabled when the game starts

- Issue: #411. Present since object LOD selection was written; noticed in an owner report on a 133-plugin load order.
- Cause: the object step looked only at a reference's own Initially Disabled flag and kept persistent ones; enable
  parents (`XESP`) were not read, so a reference kept disabled by its parent was drawn from far.
- Tests: `objects::tests::a_reference_follows_the_start_state_of_its_enable_parent_chain`,
  `objects::tests::a_later_plugin_that_disables_an_enable_parent_takes_its_children_out`,
  `objects::tests::a_persistent_disabled_reference_is_left_out_as_it_is_not_there_at_the_start` (lodwright-core).
