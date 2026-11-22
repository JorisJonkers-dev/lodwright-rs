# Far objects on a second atlas page drawn with the first atlas

- Issue: #358. Broken by #355 (commit b0a2679), which put textures on several atlas pages.
- Cause: every shape's lighting property named texture set block 3, the first shape's. A block with a shape per atlas
  page drew the second page's objects with the first page.
- Noticed by: a retake of the far-view picture set (Black Mountain dishes dark and patterned).
- Tests: `object::shapes_tests::each_shape_of_a_block_names_its_own_texture_set` (lodwright-nif) and
  `objects_on_a_second_atlas_page_are_drawn_with_their_own_texture` (lodwright-gen, `tests/atlas_pages.rs`).
