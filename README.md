# bit

A fully git-compliant implementation that currently implements a small subset of git
commands.

Wouldn't recommend using this to manipulate your valued repositories yet!

Currently implements the following commands with a subset of the
same options as git.

-   bit init,
-   bit add,
-   bit commit,
-   bit hash-object,
-   bit cat-file,
-   bit log,
-   bit ls-files,
-   bit commit-tree,
-   bit config,
-   bit write-tree,

Run commands and subcommands with the `--help` flag to see all available
options.

## Installation

Clone repository and build using the cargo package manager.

`cargo b --release`

To run, you can either use cargo as above `cargo r --release -- [<bit args>...]`.
Alternatively, you can install `bit` locally as a binary using `cargo install --path .`.
