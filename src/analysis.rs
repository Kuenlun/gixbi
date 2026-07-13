// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

//! Backend-independent branch interaction analysis.
//!
//! A branch *is* its first-parent chain (module `chains`). History of
//! branch `X` *enters* branch `Y` at the first-parent commit of `Y`
//! whose side ancestry reaches a strictly newer `X`-chain commit than
//! everything merged before (module `interactions`), whether `X` was
//! merged directly or travelled through any number of intermediate
//! branches that the graph never shows. [`analyze`] packs the
//! reachable history into a dense arena, detects those events, and
//! assembles the renderable rows, lines, edges and per-pair summaries.

mod arena;
mod chains;
mod display;
mod interactions;

use crate::error::Error;
use crate::repo::{CommitId, Repository};

/// Hard cap on input branches; memory is O(branches × commits).
pub const MAX_BRANCHES: usize = 64;

/// What to analyze and how much of it to keep.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Show every commit of the input branches, not only interactions.
    pub all: bool,
    /// Keep only the newest N rows of the graph.
    pub limit: Option<usize>,
}

/// One input branch.
#[derive(Debug)]
pub struct BranchInfo {
    /// The name exactly as the user typed it.
    pub name: String,
    /// Row of its tip commit, unless truncated away.
    pub tip: Option<usize>,
    /// Whether `HEAD` points at this branch.
    pub is_head: bool,
}

/// A displayed commit: one row of the graph, newest first.
#[derive(Debug)]
pub struct Node {
    /// Commit id.
    pub id: CommitId,
    /// Branch whose column and colour this commit uses.
    pub owner: usize,
    /// Committer time in seconds since the Unix epoch.
    pub time: i64,
    /// Committer timezone offset in seconds east of UTC.
    pub offset: i32,
    /// Summary line of the commit message.
    pub summary: String,
    /// Branches whose tip is exactly this commit.
    pub tip_of: Vec<usize>,
    /// Interaction sources this commit introduced (annotations).
    pub incoming: Vec<Incoming>,
}

/// One source of history that a merge row introduced.
#[derive(Debug)]
pub struct Incoming {
    /// Branch the history belongs to.
    pub source_branch: usize,
    /// Newest source-chain commit that entered.
    pub source: CommitId,
    /// Whether `source` is literally a parent of the merge.
    pub direct: bool,
}

/// The drawable line of one branch: its own rows plus the row on the
/// owner's line it forks off.
#[derive(Debug)]
pub struct BranchLine {
    /// Rows owned by this branch, top to bottom.
    pub rows: Vec<usize>,
    /// Row of the commit its line joins downwards, when shared.
    pub fork: Option<usize>,
    /// Per drawn segment (consecutive rows, then the fork hop): true
    /// when the commits are direct first-parent neighbours, false when
    /// history in between is elided.
    pub adjacent: Vec<bool>,
    /// The line continues below the truncation cut.
    pub cut: bool,
}

/// A drawn interaction edge between two rows.
#[derive(Debug)]
pub struct Edge {
    /// Row of the introduced source commit.
    pub source: usize,
    /// Row of the merge commit that received it.
    pub target: usize,
    /// Branch the edge carries history of (its colour).
    pub source_branch: usize,
    /// Whether the source is literally a parent of the target.
    pub direct: bool,
}

/// Latest interaction for one ordered branch pair.
#[derive(Debug)]
pub struct PairSummary {
    /// Branch the history comes from.
    pub source: usize,
    /// Branch that received it.
    pub target: usize,
    /// The most recent event, if the pair ever interacted.
    pub hit: Option<Hit>,
}

/// The event a [`PairSummary`] points at.
#[derive(Debug)]
pub struct Hit {
    /// Commit on the target chain where the history landed.
    pub merge: CommitId,
    /// Committer time of that commit.
    pub time: i64,
    /// Committer timezone offset in seconds east of UTC.
    pub offset: i32,
    /// Newest source-chain commit that entered.
    pub source: CommitId,
    /// Whether the source was a literal parent of the merge.
    pub direct: bool,
}

/// Everything the renderer needs, fully backend-independent.
#[derive(Debug)]
pub struct Analysis {
    /// Input branches, in command-line order.
    pub branches: Vec<BranchInfo>,
    /// Displayed commits, newest first.
    pub rows: Vec<Node>,
    /// One drawable line per input branch.
    pub lines: Vec<BranchLine>,
    /// Drawn interaction edges.
    pub edges: Vec<Edge>,
    /// Latest interaction per ordered branch pair.
    pub summaries: Vec<PairSummary>,
    /// Rows dropped by [`Options::limit`].
    pub truncated: usize,
}

/// Resolves `names`, walks their common history and returns the
/// renderable interaction graph.
///
/// # Errors
///
/// Returns [`Error::Resolve`] for unresolvable names,
/// [`Error::TooManyBranches`] above [`MAX_BRANCHES`], and
/// [`Error::ReadCommit`] when history is unreadable.
pub fn analyze(
    repo: &impl Repository,
    names: &[String],
    options: &Options,
) -> Result<Analysis, Error> {
    if names.len() > MAX_BRANCHES {
        return Err(Error::TooManyBranches {
            given: names.len(),
            max: MAX_BRANCHES,
        });
    }
    let ids = names
        .iter()
        .map(|name| repo.resolve(name))
        .collect::<Result<Vec<_>, _>>()?;
    let (arena, tips) = arena::Arena::load(repo, &ids)?;
    let chains = chains::Chains::build(&arena, &tips);
    let findings = interactions::detect(&arena, &chains);
    display::assemble(repo, &arena, &chains, &findings, names, &tips, options)
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::testutil::{MemRepo, id};

    /// main: 1 <- 2 <- 4 (merge of feature's 3, forked at 2).
    fn merged_repo() -> MemRepo {
        MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit_named(3, &[2], 30, "feature work")
            .commit_named(4, &[2, 3], 40, "merge feature")
            .branch("main", 4)
            .branch("feature", 3)
            .head("main")
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn sparse_graph_shows_tips_fork_and_interaction() {
        let analysis = analyze(
            &merged_repo(),
            &names(&["main", "feature"]),
            &Options::default(),
        )
        .unwrap();

        let ids: Vec<u8> = analysis
            .rows
            .iter()
            .map(|row| row.id.as_bytes()[0])
            .collect();
        assert_eq!(ids, [4, 3, 2], "newest first, ancestors below");
        assert_eq!(analysis.truncated, 0);

        assert_eq!(analysis.rows[0].tip_of, [0]);
        assert_eq!(analysis.rows[0].summary, "merge feature");
        assert_eq!(analysis.rows[1].tip_of, [1]);
        assert!(analysis.rows[2].tip_of.is_empty());

        let main = &analysis.lines[0];
        assert_eq!(
            (main.rows.as_slice(), main.fork, main.cut),
            ([0, 2].as_slice(), None, false)
        );
        assert_eq!(main.adjacent, [true]);
        let feature = &analysis.lines[1];
        assert_eq!(
            (feature.rows.as_slice(), feature.fork, feature.cut),
            ([1].as_slice(), Some(2), false)
        );
        assert_eq!(feature.adjacent, [true]);

        assert_eq!(analysis.edges.len(), 1);
        let edge = &analysis.edges[0];
        assert_eq!(
            (edge.source, edge.target, edge.source_branch, edge.direct),
            (1, 0, 1, true)
        );
        assert_eq!(analysis.rows[0].incoming.len(), 1);
        assert_eq!(analysis.rows[0].incoming[0].source, id(3));

        assert!(analysis.branches[0].is_head);
        assert!(!analysis.branches[1].is_head);
        assert_eq!(analysis.branches[0].tip, Some(0));

        let hits: Vec<bool> = analysis
            .summaries
            .iter()
            .map(|pair| pair.hit.is_some())
            .collect();
        assert_eq!(
            hits,
            [false, true],
            "(main->feature) none, (feature->main) hit"
        );
        let hit = analysis.summaries[1].hit.as_ref().unwrap();
        assert_eq!(
            (hit.merge, hit.source, hit.direct, hit.time),
            (id(4), id(3), true, 40)
        );
    }

    #[test]
    fn all_mode_includes_every_owned_commit() {
        let options = Options {
            all: true,
            ..Options::default()
        };
        let analysis = analyze(&merged_repo(), &names(&["main", "feature"]), &options).unwrap();
        let ids: Vec<u8> = analysis
            .rows
            .iter()
            .map(|row| row.id.as_bytes()[0])
            .collect();
        assert_eq!(ids, [4, 3, 2, 1]);
        assert_eq!(analysis.lines[0].rows, [0, 2, 3]);
        assert_eq!(analysis.lines[0].adjacent, [true, true]);
    }

    #[test]
    fn limit_truncates_rows_and_marks_cut_lines() {
        let options = Options {
            all: false,
            limit: Some(2),
        };
        let analysis = analyze(&merged_repo(), &names(&["main", "feature"]), &options).unwrap();
        assert_eq!(analysis.rows.len(), 2);
        assert_eq!(analysis.truncated, 1);
        assert!(analysis.lines[0].cut, "main lost its fork-side row");
        let feature = &analysis.lines[1];
        assert_eq!(
            (feature.fork, feature.cut),
            (None, true),
            "fork row was cut away"
        );
        // The edge survives: both endpoints are still visible.
        assert_eq!(analysis.edges.len(), 1);
        // Summaries ignore truncation entirely.
        assert!(analysis.summaries[1].hit.is_some());
    }

    #[test]
    fn hex_ids_and_head_names_resolve() {
        let repo = merged_repo();
        let hex = id(3).to_hex();
        let analysis = analyze(
            &repo,
            &names(&["refs/heads/main", &hex]),
            &Options::default(),
        )
        .unwrap();
        assert!(
            analysis.branches[0].is_head,
            "refs/heads/ prefix matches HEAD"
        );
        assert_eq!(analysis.branches[1].name, hex);
        assert_eq!(analysis.edges.len(), 1);
    }

    #[test]
    fn same_tip_twice_renders_one_line_and_no_interactions() {
        let analysis = analyze(
            &merged_repo(),
            &names(&["main", "main"]),
            &Options::default(),
        )
        .unwrap();
        assert_eq!(analysis.rows.len(), 1, "just the shared tip");
        assert_eq!(analysis.rows[0].tip_of, [0, 1]);
        assert!(analysis.lines[1].rows.is_empty());
        assert_eq!(analysis.lines[1].fork, None);
        assert!(analysis.edges.is_empty());
        assert!(analysis.summaries.iter().all(|pair| pair.hit.is_none()));
    }

    #[test]
    fn input_validation_errors() {
        let repo = merged_repo();
        let too_many = vec![String::from("main"); MAX_BRANCHES + 1];
        assert!(matches!(
            analyze(&repo, &too_many, &Options::default()),
            Err(Error::TooManyBranches {
                given: 65,
                max: MAX_BRANCHES
            })
        ));
        assert!(matches!(
            analyze(&repo, &names(&["main", "nope"]), &Options::default()),
            Err(Error::Resolve { name }) if name == "nope"
        ));
    }
}
