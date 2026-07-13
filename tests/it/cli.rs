// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

//! End-to-end contract of the compiled binary: exact output, colour
//! policy, exit codes and edge cases.

use crate::common::{TestRepo, gixbi, short};

/// main merges feature once; the smallest interesting graph.
fn merged() -> (TestRepo, String, String, String) {
    let repo = TestRepo::new();
    repo.commit("root");
    let base = repo.commit("base work");
    repo.checkout_new("feature", "main");
    let tip = repo.commit("feature work");
    repo.checkout("main");
    let merge = repo.merge("feature", "merge feature");
    (repo, base, tip, merge)
}

#[test]
fn plain_output_matches_exactly() {
    let (repo, base, tip, merge) = merged();
    let dir = repo.path().to_str().expect("utf8 path");
    let (stdout, stderr, code) = gixbi(&["-C", dir, "--color=never", "main", "feature"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let expected = format!(
        "\u{25cf}\u{2500}\u{256e}  {} 2026-01-01 (HEAD -> main) merge feature  \u{2190} feature @ {}\n\
         \u{2502} \u{25cf}  {} 2026-01-01 (feature) feature work\n\
         \u{25cf}\u{2500}\u{256f}  {} 2026-01-01 base work\n\
         \n\
         interactions:\n\
         \x20 main -> feature  never\n\
         \x20 feature -> main  at {} 2026-01-01, up to {}\n",
        short(&merge),
        short(&tip),
        short(&tip),
        short(&base),
        short(&merge),
        short(&tip),
    );
    assert_eq!(stdout, expected);
}

#[test]
fn color_policy_follows_flags_and_no_color() {
    let (repo, ..) = merged();
    let dir = repo.path().to_str().expect("utf8 path");

    let (always, _, _) = gixbi(&["-C", dir, "--color=always", "main", "feature"]);
    assert!(
        always.contains("\u{1b}["),
        "--color=always emits ANSI even when piped"
    );

    // Piped stdout is not a TTY: auto must stay plain.
    let (auto, _, _) = gixbi(&["-C", dir, "main", "feature"]);
    assert!(!auto.contains("\u{1b}["), "auto without TTY stays plain");

    let stripped: String = {
        let mut out = String::new();
        let mut chars = always.chars();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    };
    assert_eq!(stripped, auto, "colour never changes the content");
}

#[test]
fn ascii_mode_survives_dumb_terminals() {
    let (repo, ..) = merged();
    let dir = repo.path().to_str().expect("utf8 path");
    let (stdout, _, code) = gixbi(&["-C", dir, "--color=never", "--ascii", "main", "feature"]);
    assert_eq!(code, 0);
    assert!(stdout.starts_with("*-."), "ascii merge row");
    assert!(stdout.contains("*-'"), "ascii fork row");
    assert!(stdout.contains("<- feature @"));
    assert!(stdout.is_ascii(), "no unicode at all");
}

#[test]
fn all_and_limit_control_the_row_set() {
    let (repo, ..) = merged();
    let dir = repo.path().to_str().expect("utf8 path");

    let (sparse, _, _) = gixbi(&["-C", dir, "--color=never", "main", "feature"]);
    assert_eq!(
        sparse.lines().take_while(|line| !line.is_empty()).count(),
        3
    );

    let (all, _, _) = gixbi(&["-C", dir, "--color=never", "--all", "main", "feature"]);
    assert_eq!(
        all.lines().take_while(|line| !line.is_empty()).count(),
        4,
        "adds the root commit"
    );

    let (limited, _, _) = gixbi(&["-C", dir, "--color=never", "-n", "2", "main", "feature"]);
    assert!(limited.contains("1 older commit hidden by --limit"));
    assert!(limited.contains('\u{2506}'), "open lines end dashed");
}

#[test]
fn error_paths_use_distinct_exit_codes() {
    let (repo, ..) = merged();
    let dir = repo.path().to_str().expect("utf8 path");

    let (_, stderr, code) = gixbi(&["-C", dir, "main", "does-not-exist"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("cannot resolve 'does-not-exist' to a commit"));

    let (_, stderr, code) = gixbi(&["-C", dir, "main"]);
    assert_eq!(code, 2, "clap usage error");
    assert!(stderr.contains("2 values required"), "stderr: {stderr}");

    let outside = tempfile::TempDir::new().expect("tempdir");
    let (_, stderr, code) = gixbi(&["-C", outside.path().to_str().expect("utf8"), "a", "b"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("cannot open a git repository"));
}

#[test]
fn version_and_completions_work() {
    let (stdout, _, code) = gixbi(&["--version"]);
    assert_eq!(code, 0);
    let backend = if cfg!(feature = "gix") { "gix" } else { "git2" };
    assert!(
        stdout.trim().ends_with(&format!("({backend} backend)")),
        "version names the compiled backend: {stdout}"
    );

    let (script, _, code) = gixbi(&["--completions", "fish"]);
    assert_eq!(code, 0);
    assert!(script.contains("complete -c gixbi"));
}

#[test]
fn unicode_branch_names_and_subjects_align() {
    let repo = TestRepo::new();
    repo.commit("raíz");
    repo.checkout_new("función/ñoño", "main");
    repo.commit("trabajo con acentos áéíóú y 漢字");
    repo.checkout("main");
    repo.merge("función/ñoño", "mezcla de función");

    let dir = repo.path().to_str().expect("utf8 path");
    let (stdout, stderr, code) = gixbi(&["-C", dir, "--color=never", "main", "función/ñoño"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("(función/ñoño) trabajo con acentos áéíóú y 漢字"));
    assert!(stdout.contains("\u{2190} función/ñoño @"));

    // The summary block pads by characters, not bytes. `ñu` and `xy`
    // have the same character width but different byte lengths, so
    // byte-based padding would splay the value columns.
    repo.git(&["branch", "ñu", "main"]);
    repo.git(&["branch", "xy", "main~1"]);
    let (stdout, _, code) = gixbi(&["-C", dir, "--color=never", "main", "ñu", "xy"]);
    assert_eq!(code, 0);
    let summary = stdout
        .split("interactions:\n")
        .nth(1)
        .expect("summary block");
    let columns: Vec<usize> = summary
        .lines()
        .map(|line| {
            let chars: Vec<char> = line.chars().skip(2).collect();
            let pad = chars
                .windows(2)
                .position(|pair| pair == [' ', ' '])
                .unwrap_or_else(|| panic!("no padded label in {line}"));
            pad + chars[pad..].iter().take_while(|&&c| c == ' ').count()
        })
        .collect();
    assert_eq!(columns.len(), 6, "three branches, six ordered pairs");
    assert!(
        columns.windows(2).all(|pair| pair[0] == pair[1]),
        "value columns line up by chars: {columns:?}"
    );
}

#[test]
fn control_characters_in_messages_cannot_forge_output() {
    let repo = TestRepo::new();
    repo.commit("root");
    repo.git(&[
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "evil\rFORGED-ROW \x1b]0;pwned\x07\x1b[31mred",
    ]);
    repo.git(&["branch", "other", "HEAD~1"]);

    let dir = repo.path().to_str().expect("utf8 path");
    let (stdout, _, code) = gixbi(&["-C", dir, "--color=never", "main", "other"]);
    assert_eq!(code, 0);
    assert!(!stdout.contains('\r'), "carriage returns are stripped");
    assert!(!stdout.contains('\u{1b}'), "escape sequences are stripped");
    assert!(!stdout.contains('\u{7}'), "bells are stripped");
    assert!(
        stdout.contains("evil FORGED-ROW"),
        "printable text survives"
    );
}

#[test]
fn many_branches_render_collision_free() {
    let repo = TestRepo::new();
    repo.commit("root");
    let mut merges = Vec::new();
    for i in 0..6 {
        repo.checkout_new(&format!("b{i}"), "main");
        repo.commit(&format!("work {i}"));
        repo.checkout("main");
        merges.push(repo.merge(&format!("b{i}"), &format!("merge b{i}")));
    }
    let dir = repo.path().to_str().expect("utf8 path");
    let args: Vec<String> = ["-C", dir, "--color=never", "main"]
        .iter()
        .map(ToString::to_string)
        .chain((0..6).map(|i| format!("b{i}")))
        .collect();
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (stdout, stderr, code) = gixbi(&arg_refs);
    assert_eq!(code, 0, "stderr: {stderr}");
    for merge in &merges {
        assert!(stdout.contains(short(merge)), "every merge row is present");
    }
    assert_eq!(
        stdout
            .lines()
            .filter(|line| line.contains("-> main ") && line.contains(" at "))
            .count(),
        6,
        "each branch reports exactly one hit into main"
    );
    assert_eq!(
        stdout.matches('\u{2190}').count(),
        6,
        "each merge row draws exactly one deduplicated annotation"
    );
}

#[test]
fn broken_pipe_exits_cleanly() {
    use std::process::{Command, Stdio};

    let repo = TestRepo::new();
    repo.commit("root");
    // The read end closes before the child writes, so the very first
    // write hits EPIPE; a handful of rows is plenty.
    for i in 0..10 {
        repo.git(&[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            &format!("filler {i}"),
        ]);
    }
    repo.git(&["branch", "other", "HEAD~1"]);

    let mut child = Command::new(env!("CARGO_BIN_EXE_gixbi"))
        .args([
            "-C",
            repo.path().to_str().expect("utf8"),
            "--color=never",
            "--all",
            "main",
            "other",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn gixbi");
    drop(child.stdout.take());
    let status = child.wait().expect("wait");
    assert!(status.success(), "broken pipe is not an error");
}
