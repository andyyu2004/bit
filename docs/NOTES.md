# Naming Conventions

Functions prefixed with bit are generally higher level than the one's
without. For example, `bit_commit_tree` is meant to roughly implement
the command `git commit-tree`, while `commit_tree` is a bit lower level
and does the actual work. The stdout (printlns) will go in the `bit_`
function. The `bit` crate just does command line parsing and calls into
`libbit` to do almost everything.

Should probably remove the configuration structs inside libbit. Its more
annoying than anything else and provides a uglier api than just having
plain parameters. `bit` should figure out what to do with all the
arguments not `libbit`.
