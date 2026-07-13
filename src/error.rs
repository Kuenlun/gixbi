// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

//! The single error type surfaced by the library.

use std::path::PathBuf;

use crate::repo::CommitId;

/// Boxed source error coming from whichever git backend is compiled in.
pub type BackendError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Any failure surfaced by the gixbi library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No git repository exists at or above the requested directory.
    #[error("no git repository found at or above '{path}'")]
    Discover {
        /// Directory the discovery started from.
        path: PathBuf,
        /// Backend-specific discovery failure.
        #[source]
        source: BackendError,
    },
    /// A user-supplied name did not resolve to a commit.
    #[error("cannot resolve '{name}' to a commit")]
    Resolve {
        /// The name as the user typed it.
        name: String,
    },
    /// A commit object was missing or malformed while walking history.
    #[error("failed to read commit {id}")]
    ReadCommit {
        /// Id of the unreadable commit.
        id: CommitId,
        /// Backend-specific read failure.
        #[source]
        source: BackendError,
    },
    /// The repository stores objects with a hash other than SHA-1.
    #[error("unsupported object format '{format}' (only SHA-1 is supported)")]
    UnsupportedObjectFormat {
        /// Name of the repository's object format.
        format: String,
    },
    /// More branches were requested than the analysis supports.
    #[error("{given} branches given, at most {max} are supported")]
    TooManyBranches {
        /// How many branches the user passed.
        given: usize,
        /// The supported maximum.
        max: usize,
    },
}
