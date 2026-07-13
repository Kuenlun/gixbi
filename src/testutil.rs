// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

//! In-memory [`Repository`] for backend-free unit tests.

use std::collections::HashMap;

use crate::error::Error;
use crate::repo::{CommitDetails, CommitId, CommitMeta, Repository};

/// Builds a [`CommitId`] whose 20 bytes all equal `byte`.
pub fn id(byte: u8) -> CommitId {
    CommitId::from_bytes([byte; 20])
}

/// A purely synthetic commit graph with refs, no git involved.
#[derive(Default)]
pub struct MemRepo {
    commits: HashMap<CommitId, CommitMeta>,
    summaries: HashMap<CommitId, String>,
    refs: HashMap<String, CommitId>,
    head: Option<String>,
}

impl MemRepo {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds commit `byte` with the given parent bytes and committer time.
    #[must_use]
    pub fn commit(self, byte: u8, parents: &[u8], time: i64) -> Self {
        self.commit_named(byte, parents, time, format!("commit {byte:#04x}"))
    }

    /// Adds a commit with an explicit summary line.
    #[must_use]
    pub fn commit_named(
        mut self,
        byte: u8,
        parents: &[u8],
        time: i64,
        summary: impl Into<String>,
    ) -> Self {
        let parents = parents.iter().map(|&p| id(p)).collect();
        self.commits.insert(id(byte), CommitMeta { parents, time });
        self.summaries.insert(id(byte), summary.into());
        self
    }

    /// Adds a commit whose parent list repeats entries verbatim.
    #[must_use]
    pub fn commit_dup_parents(mut self, byte: u8, parents: &[u8], time: i64) -> Self {
        let parents = parents.iter().map(|&p| id(p)).collect();
        self.commits.insert(id(byte), CommitMeta { parents, time });
        self.summaries.insert(id(byte), String::new());
        self
    }

    /// Points `refs/heads/<name>` at commit `byte`.
    #[must_use]
    pub fn branch(mut self, name: &str, byte: u8) -> Self {
        self.refs.insert(format!("refs/heads/{name}"), id(byte));
        self
    }

    /// Marks `name` as the branch HEAD points at.
    #[must_use]
    pub fn head(mut self, name: &str) -> Self {
        self.head = Some(name.to_owned());
        self
    }
}

impl Repository for MemRepo {
    fn resolve(&self, name: &str) -> Result<CommitId, Error> {
        crate::repo::resolve_with(
            name,
            |full| self.refs.get(full).copied(),
            |id| self.commits.contains_key(&id).then_some(id),
        )
    }

    fn head_branch(&self) -> Option<String> {
        self.head.clone()
    }

    fn meta(&self, id: CommitId) -> Result<CommitMeta, Error> {
        self.commits
            .get(&id)
            .cloned()
            .ok_or_else(|| Error::ReadCommit {
                id,
                source: "missing from MemRepo".into(),
            })
    }

    fn details(&self, id: CommitId) -> Result<CommitDetails, Error> {
        let meta = self.meta(id)?;
        Ok(CommitDetails {
            summary: self.summaries.get(&id).cloned().unwrap_or_default(),
            time: meta.time,
            offset: 0,
        })
    }
}
