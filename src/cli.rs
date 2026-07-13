// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

//! Command-line surface: argument parsing, colour policy and dispatch
//! into the library pipeline.

use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use anyhow::Result;
use clap::{CommandFactory, Parser, ValueEnum};
use clap_complete::Shell;

use gixbi::analysis::{Options, analyze};
use gixbi::render::{RenderOptions, render};
use gixbi::repo;

const EXAMPLES: &str = "\
Examples:
  gixbi main feature
      Where did feature and main last meet?

  gixbi support/1.0 support/2.0 support/3.0
      Cascade check: which support branch already contains which.

  gixbi main origin/main
      How far have local and remote diverged?

  gixbi --all --limit 50 main develop
      The newest 50 rows, including every commit of both branches.";

/// Terminal graph of how chosen git branches interact.
///
/// Draws only the branches you name, newest commit first: their tips,
/// where they forked, and every point where the history of one entered
/// another, even when it travelled through branches that are not
/// shown. Below the graph, a summary answers "who last merged into
/// whom" for every pair.
#[derive(Parser)]
#[command(
    name = "gixbi",
    version = version(),
    after_long_help = EXAMPLES,
    styles = clap_cargo::style::CLAP_STYLING
)]
struct Cli {
    /// Branches to compare: local or remote branch names, tags,
    /// `HEAD`, full `refs/...` paths or 40-hex commit ids.
    #[arg(value_name = "BRANCH", num_args = 2.., required_unless_present = "completions")]
    branches: Vec<String>,

    /// Show every commit of the given branches, not only interactions.
    #[arg(short, long)]
    all: bool,

    /// Keep only the newest N graph rows.
    #[arg(short = 'n', long, value_name = "N")]
    limit: Option<usize>,

    /// Run as if started in <DIR>; the repository is discovered from
    /// there upwards.
    #[arg(short = 'C', long = "dir", value_name = "DIR", default_value = ".")]
    dir: PathBuf,

    /// Coloured output policy. `auto` follows TTY detection and the
    /// `NO_COLOR` env var; `always`/`never` override both.
    #[arg(long, value_name = "WHEN", default_value = "auto")]
    color: ColorWhen,

    /// Restrict the graph to ASCII characters.
    #[arg(long)]
    ascii: bool,

    /// Print a shell completion script for <SHELL> to stdout.
    ///
    /// Source the output to enable tab completion.
    #[arg(long, value_name = "SHELL", exclusive = true)]
    completions: Option<Shell>,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ColorWhen {
    Auto,
    Always,
    Never,
}

fn version() -> String {
    format!(
        "{} ({} backend)",
        env!("CARGO_PKG_VERSION"),
        repo::BACKEND_NAME
    )
}

/// Parses arguments, runs the pipeline and prints the report.
///
/// # Errors
///
/// Propagates repository discovery, name resolution and history read
/// failures; broken pipes are treated as success.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    if let Some(shell) = cli.completions {
        clap_complete::generate(shell, &mut Cli::command(), "gixbi", &mut io::stdout());
        return Ok(());
    }

    let repo = repo::open(&cli.dir)?;
    let options = Options {
        all: cli.all,
        limit: cli.limit,
    };
    let analysis = analyze(&repo, &cli.branches, &options)?;
    let output = render(
        &analysis,
        &RenderOptions {
            color: use_color(cli.color),
            ascii: cli.ascii,
        },
    );
    write_or_broken_pipe(&output)
}

fn use_color(when: ColorWhen) -> bool {
    auto_color(
        when,
        io::stdout().is_terminal(),
        std::env::var_os("NO_COLOR"),
    )
}

fn auto_color(when: ColorWhen, is_tty: bool, no_color: Option<std::ffi::OsString>) -> bool {
    match when {
        ColorWhen::Always => true,
        ColorWhen::Never => false,
        ColorWhen::Auto => is_tty && no_color.is_none_or(|value| value.is_empty()),
    }
}

/// Writes to stdout, treating a broken pipe (e.g. `gixbi ... | head`)
/// as a normal exit.
fn write_or_broken_pipe(output: &str) -> Result<()> {
    let mut stdout = io::stdout().lock();
    let done = stdout
        .write_all(output.as_bytes())
        .and_then(|()| stdout.flush());
    match done {
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => Ok(other?),
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn command_definition_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn version_names_the_backend() {
        let version = version();
        assert!(version.starts_with(env!("CARGO_PKG_VERSION")));
        assert!(version.ends_with("(gix backend)") || version.ends_with("(git2 backend)"));
    }

    #[test]
    fn explicit_color_policies_ignore_the_environment() {
        for (tty, no_color) in [(true, Some("1".into())), (false, None)] {
            assert!(auto_color(ColorWhen::Always, tty, no_color.clone()));
            assert!(!auto_color(ColorWhen::Never, tty, no_color));
        }
    }

    #[test]
    fn auto_needs_a_tty_and_honours_no_color() {
        assert!(auto_color(ColorWhen::Auto, true, None));
        assert!(auto_color(
            ColorWhen::Auto,
            true,
            Some(String::new().into())
        ));
        assert!(!auto_color(ColorWhen::Auto, true, Some("1".into())));
        assert!(!auto_color(ColorWhen::Auto, false, None));
    }

    #[test]
    fn two_branches_required_unless_completions() {
        assert!(Cli::try_parse_from(["gixbi", "main"]).is_err());
        assert!(Cli::try_parse_from(["gixbi"]).is_err());
        assert!(Cli::try_parse_from(["gixbi", "main", "dev"]).is_ok());
        assert!(Cli::try_parse_from(["gixbi", "--completions", "bash"]).is_ok());
        assert!(Cli::try_parse_from(["gixbi", "--completions", "bash", "main"]).is_err());
    }

    #[test]
    fn options_map_to_analysis_options() {
        let cli = Cli::try_parse_from([
            "gixbi", "-a", "-n", "7", "--ascii", "--color", "never", "a", "b",
        ])
        .unwrap();
        assert!(cli.all && cli.ascii);
        assert_eq!(cli.limit, Some(7));
        assert!(matches!(cli.color, ColorWhen::Never));
    }
}
