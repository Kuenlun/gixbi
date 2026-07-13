// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

//! [`Repository`] implementation on top of the libgit2-based `git2` backend.

use std::path::Path;

use crate::error::Error;
use crate::repo::{CommitDetails, CommitId, CommitMeta, Repository, resolve_with};

/// A repository opened through `git2`.
pub struct Git2Repo {
    inner: git2::Repository,
    /// Shallow-clone boundary commits: their recorded parents do not
    /// exist locally and must be treated as absent, like git does.
    shallow: std::collections::HashSet<CommitId>,
}

impl std::fmt::Debug for Git2Repo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Git2Repo")
            .field("git_dir", &self.inner.path())
            .finish_non_exhaustive()
    }
}

impl Git2Repo {
    /// Opens the repository containing `path`, searching upwards.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Discover`] when no repository is found.
    pub fn discover(path: &Path) -> Result<Self, Error> {
        let inner = git2::Repository::discover(path).map_err(|source| Error::Discover {
            path: path.to_owned(),
            source: source.into(),
        })?;
        let shallow = std::fs::read_to_string(inner.path().join("shallow"))
            .map(|list| list.lines().filter_map(CommitId::from_hex).collect())
            .unwrap_or_default();
        Ok(Self { inner, shallow })
    }

    fn commit(&self, id: CommitId) -> Result<git2::Commit<'_>, Error> {
        self.inner
            .find_commit(to_oid(id))
            .map_err(|source| Error::ReadCommit {
                id,
                source: source.into(),
            })
    }
}

/// Infallible for our fixed 20-byte ids; the zero oid stands in for a
/// length mismatch libgit2 cannot produce here.
fn to_oid(id: CommitId) -> git2::Oid {
    git2::Oid::from_bytes(id.as_bytes()).unwrap_or(git2::Oid::ZERO_SHA1)
}

fn from_oid(oid: git2::Oid) -> Option<CommitId> {
    oid.as_bytes().try_into().map(CommitId::from_bytes).ok()
}

impl Repository for Git2Repo {
    fn resolve(&self, name: &str) -> Result<CommitId, Error> {
        resolve_with(
            name,
            |full| {
                let commit = self
                    .inner
                    .find_reference(full)
                    .ok()?
                    .peel_to_commit()
                    .ok()?;
                from_oid(commit.id())
            },
            |id| {
                let oid = to_oid(id);
                self.inner.find_commit(oid).ok().map(|_| id)
            },
        )
    }

    fn head_branch(&self) -> Option<String> {
        if self.inner.head_detached().unwrap_or(true) {
            return None;
        }
        let head = self.inner.head().ok()?;
        head.shorthand().ok().map(ToOwned::to_owned)
    }

    fn meta(&self, id: CommitId) -> Result<CommitMeta, Error> {
        let commit = self.commit(id)?;
        let parents = if self.shallow.contains(&id) {
            Vec::new()
        } else {
            commit.parent_ids().filter_map(from_oid).collect()
        };
        Ok(CommitMeta {
            parents,
            time: commit.time().seconds(),
        })
    }

    fn details(&self, id: CommitId) -> Result<CommitDetails, Error> {
        let commit = self.commit(id)?;
        let summary =
            String::from_utf8_lossy(commit.summary_bytes().unwrap_or_default()).into_owned();
        Ok(CommitDetails {
            summary,
            time: commit.time().seconds(),
            offset: commit.time().offset_minutes() * 60,
        })
    }
}
