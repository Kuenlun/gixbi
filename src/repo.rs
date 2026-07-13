// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

//! Backend-independent access to a git repository.
//!
//! [`Repository`] is the narrow waist between the analysis (which never
//! touches git directly) and the compiled backend. Exactly one backend
//! implements it per build: [`gix`](https://docs.rs/gix) by default, or
//! [`git2`](https://docs.rs/git2) via `--no-default-features --features git2`.

use std::fmt;

use crate::error::Error;

#[cfg(feature = "git2")]
mod git2_backend;
#[cfg(feature = "gix")]
mod gix_backend;

#[cfg(feature = "git2")]
pub use git2_backend::Git2Repo;
#[cfg(feature = "gix")]
pub use gix_backend::GixRepo;

/// The backend used by [`open`] and the `gixbi` binary.
///
/// `gix` wins when both backends are compiled in, so `--all-features`
/// builds still behave like the default one.
#[cfg(feature = "gix")]
pub type DefaultRepo = GixRepo;
/// The backend used by [`open`] and the `gixbi` binary.
#[cfg(all(feature = "git2", not(feature = "gix")))]
pub type DefaultRepo = Git2Repo;

/// Name of the backend behind [`DefaultRepo`], for `--version` output.
pub const BACKEND_NAME: &str = if cfg!(feature = "gix") { "gix" } else { "git2" };

/// Open the repository containing `path` (searching upwards) with the
/// default backend.
///
/// # Errors
///
/// Returns [`Error::Discover`] when no repository is found.
#[cfg(any(feature = "gix", feature = "git2"))]
pub fn open(path: &std::path::Path) -> Result<DefaultRepo, Error> {
    DefaultRepo::discover(path)
}

/// A 20-byte SHA-1 object id, independent of the compiled backend.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommitId([u8; 20]);

impl CommitId {
    /// Wraps raw SHA-1 bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    /// The raw SHA-1 bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    /// Parses a full 40-character lowercase or uppercase hex id.
    #[must_use]
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.as_bytes();
        if hex.len() != 40 {
            return None;
        }
        let nibble = |c: u8| -> Option<u8> {
            match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            }
        };
        let mut bytes = [0_u8; 20];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = nibble(hex[2 * i])? << 4 | nibble(hex[2 * i + 1])?;
        }
        Some(Self(bytes))
    }

    /// The full 40-character lowercase hex form.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut hex = String::with_capacity(40);
        for byte in self.0 {
            for nibble in [byte >> 4, byte & 0xf] {
                hex.push(char::from_digit(u32::from(nibble), 16).unwrap_or('0'));
            }
        }
        hex
    }
}

/// Formats the full hex id; a precision like `{:.7}` shortens it.
impl fmt::Display for CommitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&self.to_hex())
    }
}

impl fmt::Debug for CommitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CommitId({self:.7})")
    }
}

/// Parents and committer timestamp: all the history walk needs.
#[derive(Debug, Clone)]
pub struct CommitMeta {
    /// Parent ids in commit order; the first parent leads the branch line.
    pub parents: Vec<CommitId>,
    /// Committer time in seconds since the Unix epoch (UTC).
    pub time: i64,
}

/// Presentation data, loaded only for the few commits that get displayed.
#[derive(Debug, Clone)]
pub struct CommitDetails {
    /// Summary line of the commit message (first paragraph, folded).
    pub summary: String,
    /// Committer time in seconds since the Unix epoch (UTC).
    pub time: i64,
    /// Committer timezone offset in seconds east of UTC.
    pub offset: i32,
}

/// Read-only view of a git repository, as narrow as the analysis allows.
pub trait Repository {
    /// Resolves a user-supplied name to a commit id.
    ///
    /// Precedence is fixed and identical across backends: `HEAD` or an
    /// explicit `refs/...` path first, then `refs/heads/<name>`,
    /// `refs/remotes/<name>`, `refs/tags/<name>` (annotated tags peeled),
    /// `refs/remotes/<name>/HEAD`, and finally a full 40-hex object id.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Resolve`] when nothing matches or the target is
    /// not a commit.
    fn resolve(&self, name: &str) -> Result<CommitId, Error>;

    /// Short name of the branch `HEAD` points at, if any.
    fn head_branch(&self) -> Option<String>;

    /// Parents and committer time; the hot path of the history walk.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ReadCommit`] when the object is missing or is not
    /// a readable commit.
    fn meta(&self, id: CommitId) -> Result<CommitMeta, Error>;

    /// Summary line and committer date of one displayed commit.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ReadCommit`] when the object is missing or is not
    /// a readable commit.
    fn details(&self, id: CommitId) -> Result<CommitDetails, Error>;
}

/// Shared name-resolution precedence over two backend primitives: an
/// exact full-name ref lookup (peeled to a commit) and a hex-id probe.
#[cfg(any(feature = "gix", feature = "git2"))]
fn resolve_with(
    name: &str,
    mut lookup: impl FnMut(&str) -> Option<CommitId>,
    probe_hex: impl FnOnce(CommitId) -> Option<CommitId>,
) -> Result<CommitId, Error> {
    let explicit = name == "HEAD" || name.starts_with("refs/");
    let found = if explicit {
        lookup(name)
    } else {
        [
            format!("refs/heads/{name}"),
            format!("refs/remotes/{name}"),
            format!("refs/tags/{name}"),
            format!("refs/remotes/{name}/HEAD"),
        ]
        .iter()
        .find_map(|full| lookup(full))
        .or_else(|| CommitId::from_hex(name).and_then(probe_hex))
    };
    found.ok_or_else(|| Error::Resolve {
        name: name.to_owned(),
    })
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    fn id(byte: u8) -> CommitId {
        CommitId::from_bytes([byte; 20])
    }

    #[test]
    fn hex_round_trip() {
        let id = CommitId::from_bytes(
            *b"\x01\x23\x45\x67\x89\xab\xcd\xef\x01\x23\x45\x67\x89\xab\xcd\xef\x01\x23\x45\x67",
        );
        assert_eq!(id.to_hex(), "0123456789abcdef0123456789abcdef01234567");
        assert_eq!(CommitId::from_hex(&id.to_hex()), Some(id));
        assert_eq!(CommitId::from_hex(&id.to_hex().to_uppercase()), Some(id));
    }

    #[test]
    fn hex_rejects_bad_input() {
        assert_eq!(CommitId::from_hex("abc"), None);
        assert_eq!(CommitId::from_hex(&"zz".repeat(20)), None);
    }

    #[test]
    fn display_supports_precision() {
        let id = CommitId::from_hex("0123456789abcdef0123456789abcdef01234567").unwrap();
        assert_eq!(format!("{id:.7}"), "0123456");
        assert_eq!(format!("{id}").len(), 40);
        assert_eq!(format!("{id:?}"), "CommitId(0123456)");
    }

    #[test]
    fn resolve_prefers_local_branches() {
        let got = resolve_with(
            "main",
            |full| (full == "refs/heads/main" || full == "refs/tags/main").then(|| id(1)),
            |_| None,
        );
        assert_eq!(got.unwrap(), id(1));
    }

    #[test]
    fn resolve_falls_through_to_remotes_tags_and_remote_head() {
        for (full, byte) in [
            ("refs/remotes/origin/dev", 2),
            ("refs/tags/v1", 3),
            ("refs/remotes/upstream/HEAD", 4),
        ] {
            let short = full
                .trim_start_matches("refs/remotes/")
                .trim_start_matches("refs/tags/")
                .trim_end_matches("/HEAD");
            let got = resolve_with(short, |name| (name == full).then(|| id(byte)), |_| None);
            assert_eq!(got.unwrap(), id(byte), "resolving {short} via {full}");
        }
    }

    #[test]
    fn resolve_explicit_paths_skip_expansion() {
        let got = resolve_with(
            "refs/heads/main",
            |full| (full == "refs/heads/main").then(|| id(7)),
            |_| None,
        );
        assert_eq!(got.unwrap(), id(7));
        assert!(resolve_with("refs/tags/nope", |_| None, |_| Some(id(9))).is_err());
        let got = resolve_with("HEAD", |full| (full == "HEAD").then(|| id(8)), |_| None);
        assert_eq!(got.unwrap(), id(8));
    }

    #[test]
    fn resolve_accepts_full_hex_when_no_ref_matches() {
        let hex = "00000000000000000000000000000000000000ff";
        let got = resolve_with(hex, |_| None, Some);
        assert_eq!(got.unwrap(), CommitId::from_hex(hex).unwrap());
        assert!(resolve_with(hex, |_| None, |_| None).is_err());
        assert!(resolve_with("not-a-thing", |_| None, |_| Some(id(1))).is_err());
    }
}
