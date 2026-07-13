// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

//! Where one branch's history enters another.
//!
//! The core quantity is `reach[s][c]`: the smallest chain position of
//! branch `s` (0 = tip) among the ancestors of commit `c`, `usize::MAX`
//! when none is reachable. It satisfies
//!
//! ```text
//! reach[s][c] = min(position of c on chain(s), min over parents of c)
//! ```
//!
//! and is computed for the whole arena in one parents-first pass.
//!
//! A commit `m` on the target chain with first parent `p` *introduces*
//! source history exactly when `reach[s][m] < reach[s][p]`: one of its
//! side parents reaches a strictly newer source-chain commit than
//! everything already present. That commit, `chain(s)[reach[s][m]]`, is
//! the source state that entered, no matter how many unshown branches
//! it travelled through. Commits sitting on both chains (shared tail)
//! are skipped: common history is not an interaction.
//!
//! `reach` also answers ancestry between two source commits in O(1):
//! `x` on `chain(s)` is an ancestor of `y` iff `reach[s][y] <= pos(x)`,
//! because the chain node at that position descends from `x` along the
//! chain. [`Findings::visual_edges`] uses this to drop, per merge, any
//! source already contained in another one (e.g. a cascade where the
//! merged branch had itself merged the first branch), and to attribute
//! shared sources to their owner, so the drawn graph stays minimal
//! while [`Findings::raw`] keeps every per-pair event for summaries.

use super::arena::Arena;
use super::chains::Chains;

/// One event: `source_branch` history reaching `target_branch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interaction {
    pub source_branch: usize,
    pub target_branch: usize,
    /// Chain commit of the target where the history landed.
    pub merge: usize,
    /// Newest source-chain commit that got introduced.
    pub source: usize,
    /// Whether `source` is literally a parent of `merge`.
    pub direct: bool,
}

/// Raw interaction events plus the reachability table they came from.
pub struct Findings {
    raw: Vec<Interaction>,
    reach: Vec<Vec<usize>>,
}

/// Detects every interaction between the given chains.
///
/// Events are emitted per target branch, newest merge first, so the
/// first event of an ordered pair is also its most recent one.
pub fn detect(arena: &Arena, chains: &Chains) -> Findings {
    let branches = chains.branch_count();
    let mut reach = vec![vec![usize::MAX; arena.len()]; branches];
    for node in arena.topo_parents_first() {
        for (branch, reach) in reach.iter_mut().enumerate() {
            let mut best = chains.position_or_max(branch, node);
            for &parent in arena.parents(node) {
                best = best.min(reach[parent]);
            }
            reach[node] = best;
        }
    }

    let mut raw = Vec::new();
    for target in 0..branches {
        for pair in chains.list(target).windows(2) {
            let (merge, first_parent) = (pair[0], pair[1]);
            for (source_branch, reach) in reach.iter().enumerate() {
                if source_branch == target || chains.position(source_branch, merge).is_some() {
                    continue;
                }
                let introduced = reach[merge];
                if introduced < reach[first_parent] {
                    let source = chains.list(source_branch)[introduced];
                    raw.push(Interaction {
                        source_branch,
                        target_branch: target,
                        merge,
                        source,
                        direct: arena.parents(merge)[1..].contains(&source),
                    });
                }
            }
        }
    }
    Findings { raw, reach }
}

impl Findings {
    /// Every per-pair event, newest first within each target branch.
    pub fn raw(&self) -> &[Interaction] {
        &self.raw
    }

    /// Is chain node `ancestor` (on `branch`) an ancestor of `node`?
    fn chain_reaches(&self, branch: usize, ancestor_position: usize, node: usize) -> bool {
        self.reach[branch][node] <= ancestor_position
    }

    /// The edges worth drawing: one per (merge, source), dominated
    /// sources dropped, labelled with the owners of both endpoints.
    pub fn visual_edges(&self, chains: &Chains) -> Vec<Interaction> {
        let mut edges: Vec<Interaction> = Vec::new();
        for event in &self.raw {
            let dominated = self.raw.iter().any(|other| {
                other.merge == event.merge
                    && other.source != event.source
                    && chains
                        .position(event.source_branch, event.source)
                        .is_some_and(|position| {
                            self.chain_reaches(event.source_branch, position, other.source)
                        })
            });
            if dominated {
                continue;
            }
            let owned = Interaction {
                source_branch: chains.owner(event.source).unwrap_or(event.source_branch),
                target_branch: chains.owner(event.merge).unwrap_or(event.target_branch),
                ..event.clone()
            };
            if !edges.contains(&owned) {
                edges.push(owned);
            }
        }
        edges.sort_by_key(|edge| (edge.merge, edge.source, edge.source_branch));
        edges
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::repo::Repository;
    use crate::testutil::{MemRepo, id};

    fn analyze(repo: &MemRepo, tips: &[u8]) -> (Arena, Chains, Findings) {
        let ids: Vec<_> = tips.iter().map(|&b| id(b)).collect();
        let (arena, _) = Arena::load(repo, &ids).unwrap();
        let tips: Vec<_> = ids.iter().map(|&i| arena.lookup(i).unwrap()).collect();
        let chains = Chains::build(&arena, &tips);
        let findings = detect(&arena, &chains);
        (arena, chains, findings)
    }

    fn simplify(arena: &Arena, events: &[Interaction]) -> Vec<(usize, usize, u8, u8, bool)> {
        events
            .iter()
            .map(|e| {
                (
                    e.source_branch,
                    e.target_branch,
                    arena.id(e.merge).as_bytes()[0],
                    arena.id(e.source).as_bytes()[0],
                    e.direct,
                )
            })
            .collect()
    }

    #[test]
    fn direct_merge_is_detected_once() {
        // main: 1 <- 2 <- 4 (merge of 3); feature: 3 forked at 2.
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit(3, &[2], 30)
            .commit(4, &[2, 3], 40);
        let (arena, chains, findings) = analyze(&repo, &[4, 3]);
        assert_eq!(simplify(&arena, findings.raw()), [(1, 0, 4, 3, true)]);
        assert_eq!(findings.visual_edges(&chains).len(), 1);
    }

    #[test]
    fn indirect_flow_through_a_hidden_branch() {
        // X: 1 <- 2 <- 3. Hidden Z: 5 <- 6 forked off X at 2.
        // Y: 4 forked at 1, then 7 = merge(4, 6).
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit(3, &[2], 30)
            .commit(4, &[1], 25)
            .commit(5, &[2], 35)
            .commit(6, &[5], 45)
            .commit(7, &[4, 6], 50);
        let (arena, _, findings) = analyze(&repo, &[3, 7]);
        // X entered Y at 7, as of X's commit 2, through Z: not direct.
        assert_eq!(simplify(&arena, findings.raw()), [(0, 1, 7, 2, false)]);
    }

    #[test]
    fn criss_cross_reports_both_directions() {
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit(0x12, &[1], 22)
            .commit(3, &[2, 0x12], 30)
            .commit(4, &[0x12, 2], 32);
        let (arena, _, findings) = analyze(&repo, &[3, 4]);
        let mut got = simplify(&arena, findings.raw());
        got.sort_unstable();
        assert_eq!(got, [(0, 1, 4, 2, true), (1, 0, 3, 0x12, true)]);
    }

    #[test]
    fn octopus_merge_reports_every_source() {
        // main 1 <- 9; a: 2; b: 3; octopus 9 = merge(1, 2, 3).
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit(3, &[1], 21)
            .commit(9, &[1, 2, 3], 40);
        let (arena, chains, findings) = analyze(&repo, &[9, 2, 3]);
        let mut got = simplify(&arena, findings.raw());
        got.sort_unstable();
        assert_eq!(got, [(1, 0, 9, 2, true), (2, 0, 9, 3, true)]);
        assert_eq!(findings.visual_edges(&chains).len(), 2);
    }

    #[test]
    fn plain_fork_yields_no_interaction() {
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit(3, &[1], 21);
        let (_, _, findings) = analyze(&repo, &[2, 3]);
        assert!(findings.raw().is_empty());
    }

    #[test]
    fn merging_an_own_ancestor_is_not_an_interaction() {
        // main: 1 <- 2 <- 3 <- M(3, 1); feature forked at 2.
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit(3, &[2], 30)
            .commit(9, &[2], 25)
            .commit(4, &[3, 1], 40);
        let (_, _, findings) = analyze(&repo, &[4, 9]);
        assert!(findings.raw().is_empty());
    }

    #[test]
    fn unrelated_roots_merged_together_are_detected() {
        // main: 1 <- 2; orphan: 8; merge 9 = (2, 8).
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit(8, &[], 15)
            .commit(9, &[2, 8], 30);
        let (arena, _, findings) = analyze(&repo, &[9, 8]);
        assert_eq!(simplify(&arena, findings.raw()), [(1, 0, 9, 8, true)]);
    }

    #[test]
    fn cascade_collapses_to_a_chain_of_edges() {
        // s1: 1 <- 2 <- 3.
        // s2: forks at 2: 0x21, then 0x22 = merge(0x21, 3).
        // s3: forks at 0x21: 0x31, then 0x32 = merge(0x31, 0x22).
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit(3, &[2], 30)
            .commit(0x21, &[2], 25)
            .commit(0x22, &[0x21, 3], 40)
            .commit(0x31, &[0x21], 35)
            .commit(0x32, &[0x31, 0x22], 50);
        let (arena, chains, findings) = analyze(&repo, &[3, 0x22, 0x32]);
        let mut raw = simplify(&arena, findings.raw());
        raw.sort_unstable();
        assert_eq!(
            raw,
            [
                (0, 1, 0x22, 3, true),    // s1 -> s2 at the first merge
                (0, 2, 0x32, 3, false),   // s1 reached s3 through s2
                (1, 2, 0x32, 0x22, true)  // s2 -> s3 at the cascade merge
            ]
        );
        // Visually the s1 content entering s3 is subsumed by the s2 edge.
        let edges = simplify(&arena, &findings.visual_edges(&chains));
        assert_eq!(edges, [(0, 1, 0x22, 3, true), (1, 2, 0x32, 0x22, true)]);
    }

    #[test]
    fn shared_source_is_attributed_to_its_owner() {
        // A: 1 <- 2 <- 3; B forks at 2; C: 4 forked at 1, merge 9 = (4, 2).
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit(3, &[2], 30)
            .commit(5, &[2], 22)
            .commit(4, &[1], 12)
            .commit(9, &[4, 2], 40);
        let (arena, chains, findings) = analyze(&repo, &[3, 5, 9]);
        let mut raw = simplify(&arena, findings.raw());
        raw.sort_unstable();
        // Both A and B genuinely reached C (their shared commit 2 did).
        assert_eq!(raw, [(0, 2, 9, 2, true), (1, 2, 9, 2, true)]);
        // But the graph draws one edge, from the owner's line.
        let edges = simplify(&arena, &findings.visual_edges(&chains));
        assert_eq!(edges, [(0, 2, 9, 2, true)]);
    }

    #[test]
    fn repeated_merges_keep_their_own_events() {
        // A: 1 <- 2 <- 3; B: 4 <- m1(4, 2) <- m2(m1, 3).
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit(3, &[2], 30)
            .commit(4, &[1], 15)
            .commit(5, &[4, 2], 25)
            .commit(6, &[5, 3], 35);
        let (arena, chains, findings) = analyze(&repo, &[3, 6]);
        let mut edges = simplify(&arena, &findings.visual_edges(&chains));
        edges.sort_unstable();
        assert_eq!(edges, [(0, 1, 5, 2, true), (0, 1, 6, 3, true)]);
    }

    #[test]
    fn fast_forwarded_branch_shares_the_whole_chain() {
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .branch("main", 2)
            .branch("old", 1);
        let main = repo.resolve("main").unwrap();
        let old = repo.resolve("old").unwrap();
        let (arena, _) = Arena::load(&repo, &[main, old]).unwrap();
        let tips = [arena.lookup(main).unwrap(), arena.lookup(old).unwrap()];
        let chains = Chains::build(&arena, &tips);
        let findings = detect(&arena, &chains);
        assert!(findings.raw().is_empty());
        assert!(findings.visual_edges(&chains).is_empty());
    }
}
