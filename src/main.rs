// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

//! Binary entry point: run the CLI and map errors to exit codes.

use std::process::ExitCode;

#[cfg(not(any(feature = "gix", feature = "git2")))]
compile_error!("select a git backend: feature `gix` (the default) or `git2`");

mod cli;

fn main() -> ExitCode {
    match cli::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
