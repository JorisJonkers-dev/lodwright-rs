# A tree texture shared by several worlds made a packed run fail

- Issue: #395. Present since packing was added in #274 (first released in 0.15.0).
- Cause: a run's file list names a tree billboard texture once per world that uses it, and the archive writer refuses
  a path it is given twice.
- Noticed by: generating a 133-plugin load order through the Toolbox, which packs by default.
- Tests: `pack::tests::a_file_that_several_worlds_list_is_packed_once` (lodwright-api).
