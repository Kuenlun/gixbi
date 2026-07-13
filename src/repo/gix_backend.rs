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
        if inner.object_hash() != gix::hash::Kind::Sha1 {
            return Err(Error::UnsupportedObjectFormat {
                format: inner.object_hash().to_string(),
            });
        }
        // Commits in packs are usually deltified against each other; a small
        // object cache saves re-inflating shared bases during the walk.
        inner.object_cache_size_if_unset(16 * 1024 * 1024);
        Ok(Self { inner })
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

const fn from_oid(oid: gix::ObjectId) -> Option<CommitId> {
    match oid {
        gix::ObjectId::Sha1(bytes) => Some(CommitId::from_bytes(bytes)),
        _ => None,
    }
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
        Some(name.as_ref().shorten().to_string())
    }

    fn meta(&self, id: CommitId) -> Result<CommitMeta, Error> {
        let object = self.commit_data(id)?;
        let mut parents = Vec::new();
        let mut time = 0;
        for token in gix::objs::CommitRefIter::from_bytes(&object.data, self.inner.object_hash()) {
            match token.map_err(|source| Error::ReadCommit {
                id,
                source: source.into(),
            })? {
                Token::Parent { id: parent } => match from_oid(parent) {
                    Some(parent) => parents.push(parent),
                    None => {
                        return Err(Error::ReadCommit {
                            id,
                            source: "non-SHA-1 parent id".into(),
                        });
                    }
                },
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
