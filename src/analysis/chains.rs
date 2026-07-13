// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

//! First-parent chains: which commits *are* each branch, and who owns
//! the history that several branches share.
//!
//! The line of a branch is its first-parent path from tip to root, the
//! same notion `git log --first-parent` uses. Two chains that meet share
//! their entire remainder (first-parent paths are unique), so ownership
//! of a shared commit goes to the earliest branch on the command line
//! and each later branch *forks off* the owner at the first shared
//! commit of its chain.

use super::arena::Arena;

const NONE: usize = usize::MAX;

/// First-parent chains of every input branch over one [`Arena`].
pub struct Chains {
    lists: Vec<Vec<usize>>,
    /// `position[branch][node]` = index within the branch chain, dense.
    position: Vec<Vec<usize>>,
    /// Earliest branch whose chain contains the node.
    owner: Vec<usize>,
    /// Per branch: first chain node owned by an earlier branch.
    fork: Vec<Option<usize>>,
}

impl Chains {
    pub fn build(arena: &Arena, tips: &[usize]) -> Self {
        let branches = tips.len();
        let mut lists = vec![Vec::new(); branches];
        let mut position = vec![vec![NONE; arena.len()]; branches];
        let mut owner = vec![NONE; arena.len()];
        for (branch, &tip) in tips.iter().enumerate() {
            let mut node = Some(tip);
            while let Some(current) = node {
                position[branch][current] = lists[branch].len();
                lists[branch].push(current);
                if owner[current] == NONE {
                    owner[current] = branch;
                }
                node = arena.first_parent(current);
            }
        }
        let fork = lists
            .iter()
            .enumerate()
            .map(|(branch, list)| list.iter().copied().find(|&node| owner[node] != branch))
            .collect();
        Self {
            lists,
            position,
            owner,
            fork,
        }
    }

    pub const fn branch_count(&self) -> usize {
        self.lists.len()
    }

    /// The full first-parent chain of `branch`, tip first.
    pub fn list(&self, branch: usize) -> &[usize] {
        &self.lists[branch]
    }

    /// Position of `node` within the chain of `branch`, 0 at the tip.
    pub fn position(&self, branch: usize, node: usize) -> Option<usize> {
        let position = self.position[branch][node];
        (position != NONE).then_some(position)
    }

    /// Raw position with `usize::MAX` for "not on the chain"; the hot
    /// form used by the interaction scan.
    pub fn position_or_max(&self, branch: usize, node: usize) -> usize {
        self.position[branch][node]
    }

    /// Earliest branch whose chain contains `node`.
    pub fn owner(&self, node: usize) -> Option<usize> {
        let owner = self.owner[node];
        (owner != NONE).then_some(owner)
    }

    /// First chain node of `branch` owned by an earlier branch: the
    /// commit where its drawn line joins the owner's line.
    pub fn fork(&self, branch: usize) -> Option<usize> {
        self.fork[branch]
    }

    /// Chain nodes owned by `branch` itself: the contiguous prefix of
    /// its chain above the fork.
    pub fn owned(&self, branch: usize) -> &[usize] {
        let list = &self.lists[branch];
        let end = self.fork[branch].map_or(list.len(), |fork| self.position[branch][fork]);
        &list[..end]
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::testutil::{MemRepo, id};

    /// main: 3 <- 2 <- 1, feature: 5 <- 4 <- 2 (forks off main at 2).
    fn forked() -> (Arena, Vec<usize>) {
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit(3, &[2], 30)
            .commit(4, &[2], 25)
            .commit(5, &[4], 35);
        let arena = Arena::load(&repo, &[id(3), id(5)]).unwrap();
        let tips = vec![arena.lookup(id(3)).unwrap(), arena.lookup(id(5)).unwrap()];
        (arena, tips)
    }

    #[test]
    fn shared_tail_is_owned_by_the_earlier_branch() {
        let (arena, tips) = forked();
        let chains = Chains::build(&arena, &tips);
        let node = |byte| arena.lookup(id(byte)).unwrap();

        assert_eq!(chains.branch_count(), 2);
        assert_eq!(chains.list(0), [node(3), node(2), node(1)]);
        assert_eq!(chains.list(1), [node(5), node(4), node(2), node(1)]);
        for shared in [2, 1] {
            assert_eq!(chains.owner(node(shared)), Some(0));
        }
        assert_eq!(chains.owner(node(5)), Some(1));
        assert_eq!(chains.fork(0), None);
        assert_eq!(chains.fork(1), Some(node(2)));
        assert_eq!(chains.owned(0), chains.list(0));
        assert_eq!(chains.owned(1), [node(5), node(4)]);
        assert_eq!(chains.position(1, node(2)), Some(2));
        assert_eq!(chains.position(0, node(5)), None);
        assert_eq!(chains.position_or_max(0, node(5)), usize::MAX);
    }

    #[test]
    fn identical_tips_leave_the_second_branch_without_owned_commits() {
        let repo = MemRepo::new().commit(1, &[], 10).commit(2, &[1], 20);
        let arena = Arena::load(&repo, &[id(2), id(2)]).unwrap();
        let tip = arena.lookup(id(2)).unwrap();
        let chains = Chains::build(&arena, &[tip, tip]);
        assert_eq!(chains.owned(1), []);
        assert_eq!(chains.fork(1), Some(tip));
        assert_eq!(chains.owner(tip), Some(0));
    }

    #[test]
    fn disjoint_roots_never_fork() {
        let repo = MemRepo::new().commit(1, &[], 10).commit(9, &[], 12);
        let arena = Arena::load(&repo, &[id(1), id(9)]).unwrap();
        let chains = Chains::build(
            &arena,
            &[arena.lookup(id(1)).unwrap(), arena.lookup(id(9)).unwrap()],
        );
        assert_eq!(chains.fork(0), None);
        assert_eq!(chains.fork(1), None);
        assert_eq!(chains.owned(1).len(), 1);
    }
}
