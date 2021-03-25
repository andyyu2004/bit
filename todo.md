- check that executable files have correct mode in trees
- avoid writing the same object to disk. e.g. object with same hash doesn't need to be rewritten
  although should debug assert that the contents are indeed identical
- validate hashes in commit-tree
