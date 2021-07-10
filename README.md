# bit

A fully git-compliant implementation that currently implements a small subset of git
commands.

Wouldn't recommend using this to manipulate your valued repositories yet!

Currently implements the following commands with a subset of the same options as git.

- bit init
- bit add
- bit branch
- bit cat-file
- bit checkout
- bit commit-tree
- bit commit
- bit config
- bit diff
- bit hash-object
- bit log
- bit ls-files
- bit status
- bit switch
- bit write-tree

Run commands and subcommands with the `--help` flag to see all available options.

## Installation

Install that latest rust nightly build using [rustup](https://rustup.rs/).

Clone repository and build using the cargo package manager.

The following should all be performed from within the cloned directory.

`cargo b --release`

To run, you can either use cargo as above `cargo r --release -- [<bit args>...]`.
Alternatively, you can install `bit` locally as a binary using `cargo install --path bit`.

# Warning

This does not currently support windows as it uses some unix specific path apis.
