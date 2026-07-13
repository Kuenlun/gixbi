// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

//! Dense in-memory storage of every commit reachable from the input tips.
//!
//! Commits are addressed by contiguous `usize` indices so the analysis
//! runs on plain arrays instead of hash lookups.

use std::collections::HashMap;

use crate::error::Error;
use crate::repo::{CommitId, Repository};

/// Every commit reachable from a set of tips, packed into parallel arrays.
pub struct Arena {
    ids: Vec<CommitId>,
    parents: Vec<Vec<usize>>,
    times: Vec<i64>,
    index: HashMap<CommitId, usize>,
}

impl Arena {
    /// Walks all ancestors of `tips` and packs them into an arena;
    /// also returns the arena index of every tip, in input order.
    ///
    /// Duplicate parent entries (tolerated by git, flagged by fsck) are
    /// collapsed so downstream passes can assume distinct parents.
    pub fn load(repo: &impl Repository, tips: &[CommitId]) -> Result<(Self, Vec<usize>), Error> {
        let mut arena = Self {
            ids: Vec::new(),
            parents: Vec::new(),
            times: Vec::new(),
            index: HashMap::new(),
        };
        let mut pending = Vec::new();
        let tip_nodes = tips
            .iter()
            .map(|&tip| arena.intern(tip, &mut pending))
            .collect();
        while let Some(node) = pending.pop() {
            let meta = repo.meta(arena.ids[node])?;
            arena.times[node] = meta.time;
            let mut parents = Vec::with_capacity(meta.parents.len());
            for parent in meta.parents {
                let parent = arena.intern(parent, &mut pending);
                if !parents.contains(&parent) {
                    parents.push(parent);
                }
            }
            arena.parents[node] = parents;
        }
        Ok((arena, tip_nodes))
    }

    fn intern(&mut self, id: CommitId, pending: &mut Vec<usize>) -> usize {
        if let Some(&node) = self.index.get(&id) {
            return node;
        }
        let node = self.ids.len();
        self.index.insert(id, node);
        self.ids.push(id);
        self.parents.push(Vec::new());
        self.times.push(0);
        pending.push(node);
        node
    }

    pub const fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn id(&self, node: usize) -> CommitId {
        self.ids[node]
    }

    pub fn parents(&self, node: usize) -> &[usize] {
        &self.parents[node]
    }

    pub fn time(&self, node: usize) -> i64 {
        self.times[node]
    }

    pub fn first_parent(&self, node: usize) -> Option<usize> {
        self.parents[node].first().copied()
    }

    /// Test-only reverse lookup; production callers keep the indices
    /// [`Self::load`] hands out.
    #[cfg(test)]
    pub fn lookup(&self, id: CommitId) -> Option<usize> {
        self.index.get(&id).copied()
    }

    /// Node indices ordered so every parent precedes all of its children.
    ///
    /// Kahn's algorithm over a compact child adjacency (CSR); the commit
    /// graph is acyclic by construction, so every node is emitted.
    pub fn topo_parents_first(&self) -> Vec<usize> {
        let len = self.len();
        let mut child_start = vec![0_usize; len + 1];
        for parents in &self.parents {
            for &parent in parents {
                child_start[parent + 1] += 1;
            }
        }
        for node in 0..len {
            child_start[node + 1] += child_start[node];
        }
        let mut children = vec![0_usize; child_start[len]];
        let mut cursor = child_start.clone();
        for (child, parents) in self.parents.iter().enumerate() {
            for &parent in parents {
                children[cursor[parent]] = child;
                cursor[parent] += 1;
            }
        }

        let mut missing: Vec<usize> = self.parents.iter().map(Vec::len).collect();
        let mut ready: Vec<usize> = (0..len).filter(|&node| missing[node] == 0).collect();
        let mut order = Vec::with_capacity(len);
        while let Some(node) = ready.pop() {
            order.push(node);
            for &child in &children[child_start[node]..child_start[node + 1]] {
                missing[child] -= 1;
                if missing[child] == 0 {
                    ready.push(child);
                }
            }
        }
        debug_assert_eq!(order.len(), len, "commit graph must be acyclic");
        order
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::testutil::{MemRepo, id};

    #[test]
    fn load_dedups_shared_history_and_duplicate_parents() {
        // 1 <- 2 <- 3 (tip a), plus 4 (tip b) whose parents are [2, 2].
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit(3, &[2], 30)
            .commit_dup_parents(4, &[2, 2], 40);
        let (arena, _) = Arena::load(&repo, &[id(3), id(4)]).unwrap();
        assert_eq!(arena.len(), 4);
        let three = arena.lookup(id(3)).unwrap();
        let four = arena.lookup(id(4)).unwrap();
        assert_eq!(arena.parents(four).len(), 1, "duplicate parents collapse");
        assert_eq!(arena.time(three), 30);
        assert_eq!(arena.id(three), id(3));
        assert_eq!(arena.first_parent(four), arena.lookup(id(2)));
        assert_eq!(arena.first_parent(arena.lookup(id(1)).unwrap()), None);
    }

    #[test]
    fn topo_orders_parents_before_children() {
        // Diamond: 1 <- {2, 3} <- 4.
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit(3, &[1], 15)
            .commit(4, &[2, 3], 30);
        let (arena, _) = Arena::load(&repo, &[id(4)]).unwrap();
        let order = arena.topo_parents_first();
        assert_eq!(order.len(), 4);
        let position: Vec<usize> = {
            let mut position = vec![0; order.len()];
            for (rank, &node) in order.iter().enumerate() {
                position[node] = rank;
            }
            position
        };
        for node in 0..arena.len() {
            for &parent in arena.parents(node) {
                assert!(position[parent] < position[node]);
            }
        }
    }

    #[test]
    fn missing_commit_surfaces_read_error() {
        let repo = MemRepo::new().commit(2, &[1], 20);
        assert!(Arena::load(&repo, &[id(2)]).is_err());
    }
}
