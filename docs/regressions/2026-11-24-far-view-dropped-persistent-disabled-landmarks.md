# The far view dropped persistent disabled landmarks

- Issue: #415. Since #412 (32dd4a7); noticed with `lodwright landmarks` on a 139-plugin load order, where Black Mountain
  dish A and the Helios One reflector were `missing` and v0.17.0 wrote both on the same data.
- Cause: #412 left out every initially disabled reference, persistent ones too, although the game's own blocks hold the
  persistent ones (`game-lod-object-rules-disabled-persistent`); the test that pinned them was changed with it.
- Tests: `objects::tests::a_persistent_disabled_reference_still_counts_as_in_the_games_own_blocks`,
  `objects::tests::a_persistent_disabled_reference_with_an_enable_parent_is_kept`,
  `objects::tests::every_reference_with_a_model_is_listed_with_the_reason_it_is_left_out_or_none` (lodwright-core).
