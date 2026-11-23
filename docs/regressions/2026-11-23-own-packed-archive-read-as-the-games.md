# The packed output of an earlier run read as the game's archive

- Issue: #392. Broken by #334 (commit 458d7bf), which leaves the game's own far-land tiles in place.
- Cause: an archive counted as the game's unless its name starts with a plugin of the request. The stub of a packed
  output is not in the request, so `Lodwright LOD.bsa` counted as the game's and its tiles were left out of the next run.
- Noticed by: the packing test of the program that links lodwright, on the bump to 0.16.0: loose, packed, loose again
  lost the four diffuse tiles.
- Tests: `shipped::tests::the_archive_of_an_earlier_packed_run_is_not_the_games` (lodwright-api).
