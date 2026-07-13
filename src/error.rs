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
    /// More branches were requested than the analysis supports.
    #[error("{given} branches given, at most {max} are supported")]
    TooManyBranches {
        /// How many branches the user passed.
        given: usize,
        /// The supported maximum.
        max: usize,
    },
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use std::error::Error as _;

    use super::*;

    #[test]
    fn messages_are_actionable_and_sources_chain() {
        let discover = Error::Discover {
            path: "/nowhere".into(),
            source: "backend detail".into(),
        };
        assert_eq!(
            discover.to_string(),
            "no git repository found at or above '/nowhere'"
        );
        assert_eq!(
            discover.source().map(ToString::to_string).as_deref(),
            Some("backend detail")
        );

        let resolve = Error::Resolve {
            name: "topic".into(),
        };
        assert_eq!(resolve.to_string(), "cannot resolve 'topic' to a commit");

        let id = CommitId::from_bytes([0xab; 20]);
        let read = Error::ReadCommit {
            id,
            source: "io".into(),
        };
        assert_eq!(read.to_string(), format!("failed to read commit {id}"));
        assert!(read.source().is_some());

        let too_many = Error::TooManyBranches { given: 65, max: 64 };
        assert_eq!(
            too_many.to_string(),
            "65 branches given, at most 64 are supported"
        );
    }
}
