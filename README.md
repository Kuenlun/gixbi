# gixbi

[![CI](https://github.com/Kuenlun/gixbi/actions/workflows/rust.yml/badge.svg?branch=master)](https://github.com/Kuenlun/gixbi/actions/workflows/rust.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

> One graph, only the branches you care about.

**gixbi** (*gix branch interaction*) draws how two or more chosen git branches interact: their tips, where they forked, and every commit where the history of one entered another, even when it travelled through branches that are not shown. Everything else in the repository stays invisible.

```
●      22fd79c 2026-01-09 (HEAD -> main) main work 2
●───╮  708387a 2026-01-08 merge support2 into main  ← support2 @ ea096ee
│ ● │  6382cc2 2026-01-07 (support1) s1 fix B
│ ├┄●  ea096ee 2026-01-06 (support2) merge temp into support2  ← support1 @ e08ffe6 (indirect)
│ ● ┆  e08ffe6 2026-01-03 s1 fix A
●─┴┄╯  8f2f56b 2026-01-02 main work 1

interactions:
  main -> support1      never
  main -> support2      never
  support1 -> main      at 708387a 2026-01-08, up to e08ffe6 (indirect)
  support1 -> support2  at ea096ee 2026-01-06, up to e08ffe6 (indirect)
  support2 -> main      at 708387a 2026-01-08, up to ea096ee
  support2 -> support1  never
```

Here `support1` never merged into `support2` directly: it flowed through a branch called `temp` that the graph does not draw. gixbi still reports the exact merge commit where it landed and the newest `support1` commit it carried, marked `(indirect)`.

Typical questions it answers at a glance:

- Did `support/1.0` already get merged into `support/2.0`, or was it the other way around?
- When did these branches last meet, and through which commit?
- Is it time for a cascade merge?

## Install

Download a binary from the [releases page](https://github.com/Kuenlun/gixbi/releases), or build from source:

```sh
cargo install --git https://github.com/Kuenlun/gixbi
```

## Usage

```sh
gixbi main feature                        # where did these two last meet?
gixbi support/1.0 support/2.0 support/3.0 # cascade check across three branches
gixbi main origin/main                    # local vs remote
gixbi --all --limit 50 main develop       # every commit of both, newest 50 rows
```

Branch arguments accept local and remote branch names, tags, `HEAD`, full `refs/...` paths and 40-hex commit ids. Useful flags:

| Flag | Effect |
|------|--------|
| `-a, --all` | Show every commit of the given branches, not only interactions |
| `-n, --limit <N>` | Keep only the newest N graph rows |
| `-C, --dir <DIR>` | Run as if started in DIR |
| `--color <WHEN>` | `auto` (TTY + `NO_COLOR`), `always`, `never` |
| `--ascii` | ASCII-only glyphs for dumb terminals |
| `--completions <SHELL>` | Print a shell completion script |

## Reading the graph

- Each input branch owns one column and one colour, in command-line order; history shared by several branches is drawn once, on the earliest branch that contains it.
- `●` is a commit; rows are newest-first. Edges into a commit show whose history it introduced, and the `← branch @ hash` annotation names the exact state that entered.
- Solid lines are direct parent links. Dashed ones mean "not direct": elided commits within a branch, or an interaction that travelled through unshown branches (`(indirect)`).
- The `interactions:` block answers "who last reached whom" for every ordered pair, including history a branch inherited by forking off later.

## How it works

A branch is its first-parent chain, the same notion `git log --first-parent` uses. For every branch `s`, gixbi computes in one pass over the commit graph the newest `s`-chain commit reachable from each commit. A commit on another branch's chain that reaches a strictly newer `s` state than its first parent is an interaction: that is where `s`'s history entered, no matter how many intermediate branches carried it. The whole analysis is a few linear scans, so hundreds of thousands of commits take about a second.

## Backends

The git backend is compile-time selectable behind one trait; the analysis never touches git directly.

```sh
cargo build --release                                      # gix (gitoxide), the default
cargo build --release --no-default-features --features git2  # libgit2 instead
```

`gixbi --version` names the backend it was built with. Both are exercised by the same test suite and produce identical output.

## Limitations

- Squash merges, rebases and cherry-picks copy content without recording ancestry, so no ancestry-based tool can see them; such interactions simply do not appear.
- SHA-256 repositories are not supported yet.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
