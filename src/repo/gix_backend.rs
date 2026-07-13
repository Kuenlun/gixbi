// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

//! [`Repository`] implementation on top of the pure-Rust `gix` backend.

use std::path::Path;

use gix::objs::commit::ref_iter::Token;

use crate::error::Error;
use crate::repo::{CommitDetails, CommitId, CommitMeta, Repository, resolve_with};

/// A repository opened through `gix`.
pub struct GixRepo {
    inner: gix::Repository,
    /// Shallow-clone boundary commits: their recorded parents do not
    /// exist locally and must be treated as absent, like git does.
    shallow: std::collections::HashSet<CommitId>,
}

impl std::fmt::Debug for GixRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GixRepo")
            .field("git_dir", &self.inner.path())
            .finish_non_exhaustive()
    }
}

impl GixRepo {
    /// Opens the repository containing `path`, searching upwards.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Discover`] when no repository is found.
    pub fn discover(path: &Path) -> Result<Self, Error> {
        let mut inner = gix::discover(path).map_err(|source| Error::Discover {
            path: path.to_owned(),
            source: source.into(),
        })?;
        // Commits in packs are usually deltified against each other; a small
        // object cache saves re-inflating shared bases during the walk.
        inner.object_cache_size_if_unset(16 * 1024 * 1024);
        let shallow = inner
            .shallow_commits()
            .ok()
            .flatten()
            .map(|commits| commits.iter().copied().filter_map(from_oid).collect())
            .unwrap_or_default();
        Ok(Self { inner, shallow })
    }

    fn commit_data(&self, id: CommitId) -> Result<gix::Object<'_>, Error> {
        let object = self
            .inner
            .find_object(to_oid(id))
            .map_err(|source| Error::ReadCommit {
                id,
                source: source.into(),
            })?;
        if object.kind == gix::object::Kind::Commit {
            Ok(object)
        } else {
            Err(Error::ReadCommit {
                id,
                source: format!("object is a {}, not a commit", object.kind).into(),
            })
        }
    }
}

const fn to_oid(id: CommitId) -> gix::ObjectId {
    gix::ObjectId::Sha1(*id.as_bytes())
}

fn from_oid(oid: gix::ObjectId) -> Option<CommitId> {
    oid.as_slice().try_into().map(CommitId::from_bytes).ok()
}

impl Repository for GixRepo {
    fn resolve(&self, name: &str) -> Result<CommitId, Error> {
        resolve_with(
            name,
            |full| {
                let mut reference = self.inner.find_reference(full).ok()?;
                let commit = reference.peel_to_commit().ok()?;
                from_oid(commit.id)
            },
            |id| {
                let object = self.inner.find_object(to_oid(id)).ok()?;
                (object.kind == gix::object::Kind::Commit).then_some(id)
            },
        )
    }

    fn head_branch(&self) -> Option<String> {
        let name = self.inner.head_name().ok().flatten()?;
        // Unborn branches decorate nothing, matching git2 and git log.
        self.inner.head_id().ok()?;
        Some(name.as_ref().shorten().to_string())
    }

    fn meta(&self, id: CommitId) -> Result<CommitMeta, Error> {
        let object = self.commit_data(id)?;
        let grafted = self.shallow.contains(&id);
        let mut parents = Vec::new();
        let mut time = 0;
        for token in gix::objs::CommitRefIter::from_bytes(&object.data, self.inner.object_hash()) {
            match token.map_err(|source| Error::ReadCommit {
                id,
                source: source.into(),
            })? {
                Token::Parent { id: parent } if !grafted => parents.extend(from_oid(parent)),
                Token::Committer { signature } => {
                    time = signature.seconds();
                    // Parents always precede the committer line, so the
                    // rest of the header is irrelevant here.
                    break;
                }
                _ => {}
            }
        }
        Ok(CommitMeta { parents, time })
    }

    fn details(&self, id: CommitId) -> Result<CommitDetails, Error> {
        let object = self.commit_data(id)?;
        let commit = gix::objs::CommitRef::from_bytes(&object.data, self.inner.object_hash())
            .map_err(|source| Error::ReadCommit {
                id,
                source: source.into(),
            })?;
        let time = commit
            .committer()
            .ok()
            .and_then(|sig| sig.time().ok())
            .unwrap_or_default();
        Ok(CommitDetails {
            summary: commit.message().summary().to_string(),
            time: time.seconds,
            offset: time.offset,
        })
    }
}
