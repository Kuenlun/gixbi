// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

//! Shared fixtures: hermetic git repositories built with the real git
//! CLI, deterministic timestamps, and helpers to drive both the
//! library (against every compiled backend) and the binary.

use std::cell::Cell;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use gixbi::repo::Repository;
use tempfile::TempDir;

/// A throwaway git repository with reproducible commits.
pub struct TestRepo {
    dir: TempDir,
    tick: Cell<u64>,
}

/// Seconds of 2026-01-01 00:00:00 UTC; every commit advances one hour.
const EPOCH: u64 = 1_767_225_600;

impl TestRepo {
    pub fn new() -> Self {
        let repo = Self {
            dir: TempDir::new().unwrap_or_else(|error| panic!("create tempdir: {error}")),
            tick: Cell::new(0),
        };
        repo.git(&["init", "-q", "-b", "main"]);
        repo
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Runs git with a hermetic environment and deterministic identity
    /// and dates; panics on failure.
    pub fn git(&self, args: &[&str]) -> String {
        let output = self.git_raw(args);
        assert!(
            output.status.success(),
            "git {args:?} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_owned()
    }

    /// Same as [`Self::git`] but hands back the raw output.
    pub fn git_raw(&self, args: &[&str]) -> Output {
        let stamp = format!("@{} +0000", EPOCH + 3600 * self.tick.get());
        self.tick.set(self.tick.get() + 1);
        Command::new("git")
            .arg("-C")
            .arg(self.dir.path())
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .env("GIT_AUTHOR_DATE", &stamp)
            .env("GIT_COMMITTER_DATE", &stamp)
            .output()
            .unwrap_or_else(|error| panic!("spawn git: {error}"))
    }

    /// Empty commit with the given subject; returns its full hash.
    pub fn commit(&self, message: &str) -> String {
        self.git(&["commit", "-q", "--allow-empty", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }

    pub fn checkout_new(&self, name: &str, from: &str) -> &Self {
        self.git(&["checkout", "-q", "-b", name, from]);
        self
    }

    pub fn checkout(&self, name: &str) -> &Self {
        self.git(&["checkout", "-q", name]);
        self
    }

    /// `--no-ff` merge; returns the merge commit hash.
    pub fn merge(&self, other: &str, message: &str) -> String {
        self.git(&["merge", "-q", "--no-ff", "-m", message, other]);
        self.git(&["rev-parse", "HEAD"])
    }

    /// Octopus merge of several heads; returns the merge commit hash.
    pub fn merge_octopus(&self, others: &[&str], message: &str) -> String {
        let mut args = vec!["merge", "-q", "--no-ff", "-m", message];
        args.extend_from_slice(others);
        self.git(&args);
        self.git(&["rev-parse", "HEAD"])
    }

    /// Squash-merge followed by a normal commit: content flows, but no
    /// ancestry link is recorded.
    pub fn merge_squash(&self, other: &str, message: &str) -> String {
        self.git(&["merge", "-q", "--squash", other]);
        self.commit(message)
    }

    pub fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }
}

/// Short hash as gixbi prints it.
pub fn short(hash: &str) -> &str {
    &hash[..7]
}

/// Runs `check` against the repository at `path` once per compiled
/// backend, so every scenario exercises gix and git2 identically.
pub fn each_backend(path: &Path, mut check: impl FnMut(&dyn Repository, &str)) {
    #[cfg(feature = "gix")]
    check(
        &gixbi::repo::GixRepo::discover(path)
            .unwrap_or_else(|error| panic!("open with gix: {error}")),
        "gix",
    );
    #[cfg(feature = "git2")]
    check(
        &gixbi::repo::Git2Repo::discover(path)
            .unwrap_or_else(|error| panic!("open with git2: {error}")),
        "git2",
    );
}

/// Runs the compiled gixbi binary; returns (stdout, stderr, exit code).
pub fn gixbi(args: &[&str]) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_gixbi"))
        .args(args)
        .env_remove("NO_COLOR")
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|error| panic!("spawn gixbi: {error}"));
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().unwrap_or(-1),
    )
}
