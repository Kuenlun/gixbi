// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

//! Turns the raw analysis into the renderable [`Analysis`]: which
//! commits become rows, in what order, and how lines, edges and
//! summaries map onto them.
//!
//! Row order is a topological sort of the displayed subgraph (every
//! drawn connection goes from a descendant down to an ancestor) that
//! greedily pops the newest ready commit, so rows read newest-first
//! even across skewed committer clocks.

use std::collections::{BTreeSet, BinaryHeap, HashMap};

use super::arena::Arena;
use super::chains::Chains;
use super::interactions::{Findings, Interaction};
use super::{Analysis, BranchInfo, BranchLine, Edge, Hit, Incoming, Node, Options, PairSummary};
use crate::error::Error;
use crate::repo::Repository;

pub fn assemble(
    repo: &impl Repository,
    arena: &Arena,
    chains: &Chains,
    findings: &Findings,
    names: &[String],
    tips: &[usize],
    options: &Options,
) -> Result<Analysis, Error> {
    let visual = findings.visual_edges(chains);
    let selected = select(chains, tips, &visual, options.all);
    let line_nodes: Vec<Vec<usize>> = (0..names.len())
        .map(|branch| {
            chains
                .owned(branch)
                .iter()
                .copied()
                .filter(|node| selected.contains(node))
                .collect()
        })
        .collect();

    let mut order = row_order(arena, chains, &selected, &line_nodes, &visual);
    let kept = options
        .limit
        .map_or(order.len(), |limit| limit.min(order.len()));
    let truncated = order.len() - kept;
    order.truncate(kept);
    let row_of: HashMap<usize, usize> = order
        .iter()
        .enumerate()
        .map(|(row, &node)| (node, row))
        .collect();

    let mut rows = build_rows(repo, arena, chains, &order)?;
    let lines = build_lines(chains, &order, &row_of, &line_nodes);
    let edges = attach_edges(arena, &visual, &row_of, &mut rows);
    let branches = describe_branches(repo, names, tips, &row_of, &mut rows);
    let summaries = build_summaries(repo, arena, findings, names.len())?;

    Ok(Analysis {
        branches,
        rows,
        lines,
        edges,
        summaries,
        truncated,
    })
}

/// The commits that become rows.
fn select(chains: &Chains, tips: &[usize], visual: &[Interaction], all: bool) -> BTreeSet<usize> {
    let mut selected: BTreeSet<usize> = tips.iter().copied().collect();
    if all {
        for branch in 0..chains.branch_count() {
            selected.extend(chains.owned(branch));
        }
    } else {
        for event in visual {
            selected.insert(event.merge);
            selected.insert(event.source);
        }
    }
    selected.extend((0..chains.branch_count()).filter_map(|branch| chains.fork(branch)));
    selected
}

/// Newest-first topological order over the selected commits.
///
/// Constraints mirror exactly what gets drawn: line segments, fork
/// hops and interaction edges, each pointing at an ancestor.
fn row_order(
    arena: &Arena,
    chains: &Chains,
    selected: &BTreeSet<usize>,
    line_nodes: &[Vec<usize>],
    visual: &[Interaction],
) -> Vec<usize> {
    let mut blockers: HashMap<usize, usize> = HashMap::new();
    let mut below: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut constrain = |upper: usize, lower: usize| {
        *blockers.entry(lower).or_insert(0) += 1;
        below.entry(upper).or_default().push(lower);
    };
    for (branch, nodes) in line_nodes.iter().enumerate() {
        for pair in nodes.windows(2) {
            constrain(pair[0], pair[1]);
        }
        if let (Some(&last), Some(fork)) = (nodes.last(), chains.fork(branch)) {
            constrain(last, fork);
        }
    }
    for event in visual {
        constrain(event.merge, event.source);
    }

    let mut ready: BinaryHeap<(i64, usize)> = selected
        .iter()
        .copied()
        .filter(|node| !blockers.contains_key(node))
        .map(|node| (arena.time(node), node))
        .collect();
    let mut order: Vec<usize> = Vec::with_capacity(selected.len());
    while let Some((_, node)) = ready.pop() {
        order.push(node);
        for &lower in below.get(&node).map_or(&[][..], Vec::as_slice) {
            if let Some(pending) = blockers.get_mut(&lower) {
                *pending -= 1;
                if *pending == 0 {
                    ready.push((arena.time(lower), lower));
                }
            }
        }
    }
    order
}

/// One presentable [`Node`] per kept row.
fn build_rows(
    repo: &impl Repository,
    arena: &Arena,
    chains: &Chains,
    order: &[usize],
) -> Result<Vec<Node>, Error> {
    order
        .iter()
        .map(|&node| {
            let details = repo.details(arena.id(node))?;
            // Commit messages are attacker-controlled terminal input:
            // strip control characters (\r, ESC, ...) so a summary can
            // never rewrite the graph or retitle the terminal.
            let summary = details
                .summary
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect();
            Ok(Node {
                id: arena.id(node),
                owner: chains.owner(node).unwrap_or_default(),
                time: details.time,
                offset: details.offset,
                summary,
                tip_of: Vec::new(),
                incoming: Vec::new(),
            })
        })
        .collect()
}

/// Branch lines over kept rows. Kept rows are always a prefix of the
/// selected line, because constraints force uppers out first.
fn build_lines(
    chains: &Chains,
    order: &[usize],
    row_of: &HashMap<usize, usize>,
    line_nodes: &[Vec<usize>],
) -> Vec<BranchLine> {
    line_nodes
        .iter()
        .enumerate()
        .map(|(branch, all_nodes)| {
            let rows: Vec<usize> = all_nodes
                .iter()
                .copied()
                .map_while(|node| row_of.get(&node).copied())
                .collect();
            let wanted_fork = rows.last().and_then(|_| chains.fork(branch));
            let fork = wanted_fork
                .and_then(|fork| row_of.get(&fork).copied())
                .map(|fork_row| {
                    let last = rows[rows.len() - 1];
                    (
                        fork_row,
                        chain_adjacent(chains, branch, order, last, fork_row),
                    )
                });
            let adjacent: Vec<bool> = rows
                .windows(2)
                .map(|pair| chain_adjacent(chains, branch, order, pair[0], pair[1]))
                .collect();
            let drawn = rows.len() + usize::from(fork.is_some());
            let wanted = all_nodes.len() + usize::from(wanted_fork.is_some());
            let cut = drawn < wanted;
            BranchLine {
                rows,
                fork,
                adjacent,
                cut,
            }
        })
        .collect()
}

/// Drawn edges, plus the annotation every visual event leaves on its
/// merge row even when its source fell below the truncation cut.
fn attach_edges(
    arena: &Arena,
    visual: &[Interaction],
    row_of: &HashMap<usize, usize>,
    rows: &mut [Node],
) -> Vec<Edge> {
    let mut edges = Vec::new();
    for event in visual {
        let Some(&target) = row_of.get(&event.merge) else {
            continue;
        };
        rows[target].incoming.push(Incoming {
            source_branch: event.source_branch,
            source: arena.id(event.source),
            direct: event.direct,
        });
        if let Some(&source) = row_of.get(&event.source) {
            edges.push(Edge {
                source,
                target,
                source_branch: event.source_branch,
                direct: event.direct,
            });
        }
    }
    edges
}

/// Branch descriptors and tip decorations.
fn describe_branches(
    repo: &impl Repository,
    names: &[String],
    tips: &[usize],
    row_of: &HashMap<usize, usize>,
    rows: &mut [Node],
) -> Vec<BranchInfo> {
    let head = repo.head_branch();
    let branches: Vec<BranchInfo> = names
        .iter()
        .zip(tips)
        .map(|(name, &tip)| BranchInfo {
            name: name.clone(),
            tip: row_of.get(&tip).copied(),
            is_head: is_head(name, head.as_deref()),
        })
        .collect();
    for (branch, info) in branches.iter().enumerate() {
        if let Some(row) = info.tip {
            rows[row].tip_of.push(branch);
        }
    }
    branches
}

/// Latest raw event per ordered pair, independent of truncation.
fn build_summaries(
    repo: &impl Repository,
    arena: &Arena,
    findings: &Findings,
    branches: usize,
) -> Result<Vec<PairSummary>, Error> {
    let mut merge_details: HashMap<usize, (i64, i32)> = HashMap::new();
    let mut summaries = Vec::with_capacity(branches.saturating_sub(1) * branches);
    for source in 0..branches {
        for target in (0..branches).filter(|&target| target != source) {
            let event = findings
                .raw()
                .iter()
                .find(|event| event.source_branch == source && event.target_branch == target);
            let hit = event
                .map(|event| {
                    let (time, offset) = if let Some(&cached) = merge_details.get(&event.merge) {
                        cached
                    } else {
                        let details = repo.details(arena.id(event.merge))?;
                        let value = (details.time, details.offset);
                        merge_details.insert(event.merge, value);
                        value
                    };
                    Ok::<Hit, Error>(Hit {
                        merge: arena.id(event.merge),
                        time,
                        offset,
                        source: arena.id(event.source),
                        direct: event.direct,
                    })
                })
                .transpose()?;
            summaries.push(PairSummary {
                source,
                target,
                hit,
            });
        }
    }
    Ok(summaries)
}

/// Are two kept rows directly linked on the chain of `branch`?
fn chain_adjacent(
    chains: &Chains,
    branch: usize,
    order: &[usize],
    upper_row: usize,
    lower_row: usize,
) -> bool {
    let upper = chains.position(branch, order[upper_row]);
    let lower = chains.position(branch, order[lower_row]);
    matches!((upper, lower), (Some(u), Some(l)) if l == u + 1)
}

fn is_head(name: &str, head: Option<&str>) -> bool {
    name == "HEAD"
        || head.is_some_and(|head| name == head || name.strip_prefix("refs/heads/") == Some(head))
}
