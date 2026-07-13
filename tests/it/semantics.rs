// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

//! Semantic contract of the analysis against real git repositories,
//! exercised through every compiled backend.

use crate::common::{TestRepo, each_backend, short};
use gixbi::analysis::{Analysis, Options, analyze};

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

fn run(repo: &TestRepo, branches: &[&str]) -> Vec<Analysis> {
    let mut results = Vec::new();
    each_backend(repo.path(), |backend, _| {
        results.push(
            analyze(&backend, &names(branches), &Options::default())
                .unwrap_or_else(|error| panic!("analyze: {error}")),
        );
    });
    results
}

/// The last hit for (source -> target), as short hashes.
fn last_hit(analysis: &Analysis, source: usize, target: usize) -> Option<(String, String, bool)> {
    analysis
        .summaries
        .iter()
        .find(|pair| pair.source == source && pair.target == target)
        .and_then(|pair| pair.hit.as_ref())
        .map(|hit| {
            (
                format!("{:.7}", hit.merge),
                format!("{:.7}", hit.source),
                hit.direct,
            )
        })
}

#[test]
fn direct_merge_is_reported_with_exact_commits() {
    let repo = TestRepo::new();
    repo.commit("root");
    let base = repo.commit("base");
    repo.checkout_new("feature", "main");
    let tip = repo.commit("feature work");
    repo.checkout("main");
    let merge = repo.merge("feature", "merge feature");

    for analysis in run(&repo, &["main", "feature"]) {
        assert_eq!(
            last_hit(&analysis, 1, 0),
            Some((short(&merge).to_owned(), short(&tip).to_owned(), true))
        );
        assert_eq!(last_hit(&analysis, 0, 1), None);
        assert_eq!(analysis.edges.len(), 1);
        let (fork_row, fork_adjacent) = analysis.lines[1].fork.expect("feature forks off main");
        assert_eq!(format!("{:.7}", analysis.rows[fork_row].id), short(&base));
        assert!(fork_adjacent, "tip sits right on the fork");
    }
}

#[test]
fn interaction_survives_deleting_the_intermediate_branch() {
    let repo = TestRepo::new();
    repo.commit("root");
    let base = repo.commit("x base");
    let carried = repo.commit("x carried state");
    repo.checkout_new("temp", "main");
    repo.commit("temp work");
    repo.checkout_new("y", &base);
    repo.commit("y own work");
    let merge = repo.merge("temp", "merge temp into y");
    repo.git(&["branch", "-D", "temp"]);
    repo.checkout("main");
    repo.commit("x moves on");

    for analysis in run(&repo, &["main", "y"]) {
        let (at, up_to, direct) = last_hit(&analysis, 0, 1).expect("x reached y through temp");
        assert_eq!(at, short(&merge));
        assert_eq!(up_to, short(&carried));
        assert!(!direct, "the flow went through a (deleted) branch");
    }
}

#[test]
fn cascade_keeps_summaries_but_collapses_edges() {
    let repo = TestRepo::new();
    repo.commit("root");
    repo.commit("s1 base");
    repo.checkout_new("s2", "main");
    repo.commit("s2 work");
    repo.checkout("main");
    let s1_state = repo.commit("s1 advance");
    repo.checkout("s2");
    let first = repo.merge("main", "merge s1 into s2");
    repo.checkout_new("s3", "main~1");
    repo.commit("s3 work");
    let second = repo.merge("s2", "merge s2 into s3");

    for analysis in run(&repo, &["main", "s2", "s3"]) {
        // Raw truth: s1 reached s3, indirectly.
        let (at, up_to, direct) = last_hit(&analysis, 0, 2).expect("main reached s3");
        assert_eq!(
            (at, up_to, direct),
            (
                short(&second).to_owned(),
                short(&s1_state).to_owned(),
                false
            )
        );
        assert_eq!(
            last_hit(&analysis, 1, 2),
            Some((short(&second).to_owned(), short(&first).to_owned(), true))
        );
        // Drawn edges: one per merge, the cascade edge subsumes main's.
        assert_eq!(analysis.edges.len(), 2);
        let annotations: usize = analysis.rows.iter().map(|row| row.incoming.len()).sum();
        assert_eq!(annotations, 2);
    }
}

#[test]
fn squash_merge_records_no_ancestry_interaction() {
    let repo = TestRepo::new();
    repo.commit("root");
    repo.git(&["commit", "-q", "--allow-empty", "-m", "base"]);
    repo.checkout_new("feature", "main");
    std::fs::write(repo.path().join("f.txt"), "content\n").expect("write file");
    repo.git(&["add", "f.txt"]);
    repo.commit("feature file");
    repo.checkout("main");
    repo.merge_squash("feature", "squashed feature");

    for analysis in run(&repo, &["main", "feature"]) {
        assert!(
            analysis.summaries.iter().all(|pair| pair.hit.is_none()),
            "squash leaves no ancestry trace by design"
        );
        assert!(analysis.edges.is_empty());
    }
}

#[test]
fn fast_forward_and_shared_tips_share_one_line() {
    let repo = TestRepo::new();
    repo.commit("root");
    repo.commit("work");
    repo.git(&["branch", "alias"]);
    repo.git(&["branch", "behind", "HEAD~1"]);

    for analysis in run(&repo, &["main", "alias", "behind"]) {
        assert_eq!(analysis.rows.len(), 2, "tip row and behind row only");
        assert_eq!(
            analysis.rows[0].tip_of,
            [0, 1],
            "main and alias share the tip"
        );
        assert_eq!(analysis.rows[1].tip_of, [2]);
        assert!(analysis.lines[1].rows.is_empty(), "alias owns nothing");
        assert!(analysis.lines[2].rows.is_empty(), "behind owns nothing");
        assert!(analysis.edges.is_empty());
        assert!(analysis.summaries.iter().all(|pair| pair.hit.is_none()));
    }
}

#[test]
fn octopus_merges_attribute_every_head() {
    let repo = TestRepo::new();
    repo.commit("root");
    repo.checkout_new("a", "main");
    let a_tip = repo.commit("a work");
    repo.checkout_new("b", "main");
    let b_tip = repo.commit("b work");
    repo.checkout("main");
    let merge = repo.merge_octopus(&["a", "b"], "octopus");

    for analysis in run(&repo, &["main", "a", "b"]) {
        assert_eq!(
            last_hit(&analysis, 1, 0),
            Some((short(&merge).to_owned(), short(&a_tip).to_owned(), true))
        );
        assert_eq!(
            last_hit(&analysis, 2, 0),
            Some((short(&merge).to_owned(), short(&b_tip).to_owned(), true))
        );
        assert_eq!(analysis.edges.len(), 2, "one edge per octopus head");
    }
}

#[test]
fn criss_cross_reports_both_directions() {
    let repo = TestRepo::new();
    repo.commit("root");
    let a_state = repo.commit("a work");
    repo.checkout_new("b", "main~1");
    let b_state = repo.commit("b work");
    let b_merge = repo.merge("main", "b merges a");
    repo.checkout("main");
    let a_merge = repo.merge(&b_state, "a merges b pre-cross");

    for analysis in run(&repo, &["main", "b"]) {
        assert_eq!(
            last_hit(&analysis, 1, 0),
            Some((short(&a_merge).to_owned(), short(&b_state).to_owned(), true))
        );
        assert_eq!(
            last_hit(&analysis, 0, 1),
            Some((short(&b_merge).to_owned(), short(&a_state).to_owned(), true))
        );
    }
}

#[test]
fn names_resolve_like_git_across_ref_kinds() {
    let repo = TestRepo::new();
    repo.commit("root");
    let tagged = repo.commit("tagged state");
    repo.git(&["tag", "-a", "v1", "-m", "annotated tag"]);
    repo.checkout_new("feature", "main");
    repo.commit("feature work");
    repo.git(&["update-ref", "refs/remotes/origin/feature", "feature"]);
    repo.checkout("main");
    repo.commit("newer main");

    for analysis in run(
        &repo,
        &["refs/heads/main", "v1", "origin/feature", &repo.head()],
    ) {
        assert_eq!(analysis.branches.len(), 4);
        let tag_tip = analysis.branches[1].tip.expect("tag resolves");
        assert_eq!(format!("{:.7}", analysis.rows[tag_tip].id), short(&tagged));
    }

    each_backend(repo.path(), |backend, name| {
        let err = analyze(&backend, &names(&["main", "no-such"]), &Options::default()).unwrap_err();
        assert!(
            err.to_string().contains("no-such"),
            "{name}: unresolvable names carry the input"
        );
    });
}

#[test]
fn bare_repositories_work() {
    let repo = TestRepo::new();
    repo.commit("root");
    repo.checkout_new("feature", "main");
    repo.commit("feature work");
    repo.checkout("main");
    repo.merge("feature", "merge feature");

    let bare = tempfile::TempDir::new().expect("tempdir");
    let bare_path = bare.path().join("clone.git");
    repo.git(&[
        "clone",
        "-q",
        "--bare",
        ".",
        bare_path.to_str().expect("utf8 path"),
    ]);

    each_backend(&bare_path, |backend, name| {
        let analysis = analyze(&backend, &names(&["main", "feature"]), &Options::default())
            .expect("analyze bare");
        assert_eq!(analysis.edges.len(), 1, "{name}: bare repo analyzes fine");
    });
}

#[test]
fn detached_head_disables_the_head_marker() {
    let repo = TestRepo::new();
    repo.commit("root");
    repo.commit("work");
    repo.git(&["branch", "other", "HEAD~1"]);
    repo.git(&["checkout", "-q", "--detach", "HEAD"]);

    for analysis in run(&repo, &["main", "other"]) {
        assert!(analysis.branches.iter().all(|branch| !branch.is_head));
    }
}

#[test]
fn shallow_clones_analyze_what_is_visible() {
    let repo = TestRepo::new();
    repo.commit("root");
    for i in 0..5 {
        repo.commit(&format!("main {i}"));
    }
    repo.checkout_new("feature", "main");
    repo.commit("feature work");
    repo.checkout("main");
    repo.merge("feature", "merge feature");

    let shallow = tempfile::TempDir::new().expect("tempdir");
    let shallow_path = shallow.path().join("shallow");
    repo.git(&[
        "clone",
        "-q",
        "--depth",
        "2",
        "--no-single-branch",
        &format!("file://{}", repo.path().display()),
        shallow_path.to_str().expect("utf8 path"),
    ]);

    each_backend(&shallow_path, |backend, name| {
        let analysis = analyze(
            &backend,
            &names(&["origin/main", "origin/feature"]),
            &Options::default(),
        )
        .unwrap_or_else(|error| panic!("{name}: shallow analysis failed: {error}"));
        assert!(
            !analysis.rows.is_empty(),
            "{name}: truncated history still renders"
        );
    });
}

#[test]
fn sha256_repositories_are_rejected_cleanly() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let out = std::process::Command::new("git")
        .args(["init", "-q", "-b", "main", "--object-format=sha256"])
        .arg(dir.path())
        .output()
        .expect("git init");
    assert!(out.status.success());

    each_backend_open_error(dir.path());
}

/// Opening (or analyzing) must fail with a clear error, never panic.
fn each_backend_open_error(path: &std::path::Path) {
    // Neither backend supports SHA-256 repositories today: opening (or,
    // depending on build options, resolving) must fail cleanly.
    #[cfg(feature = "gix")]
    {
        let result = gixbi::repo::GixRepo::discover(path)
            .and_then(|repo| analyze(&&repo, &names(&["main", "main"]), &Options::default()));
        assert!(result.is_err(), "gix: sha256 analysis cannot succeed");
    }
    #[cfg(feature = "git2")]
    {
        let result = gixbi::repo::Git2Repo::discover(path)
            .and_then(|repo| analyze(&&repo, &names(&["main", "main"]), &Options::default()));
        assert!(result.is_err(), "git2: sha256 analysis cannot succeed");
    }
}

#[test]
fn empty_and_unborn_repositories_error_cleanly() {
    let repo = TestRepo::new();
    each_backend(repo.path(), |backend, name| {
        let error = analyze(&backend, &names(&["main", "main"]), &Options::default()).unwrap_err();
        assert!(
            matches!(error, gixbi::Error::Resolve { .. }),
            "{name}: unborn branch cannot resolve"
        );
    });
}

#[test]
fn backend_error_paths_are_clean() {
    use gixbi::repo::CommitId;

    let repo = TestRepo::new();
    repo.commit("root");
    let tree_hex = repo.git(&["show", "-s", "--format=%T", "HEAD"]);
    let tree = CommitId::from_hex(&tree_hex).expect("valid hex");

    // A structurally broken commit object, stored verbatim.
    std::fs::write(repo.path().join("junk"), b"tree not-a-hash\ngarbage\n").expect("write junk");
    let broken_hex = repo.git(&["hash-object", "-w", "-t", "commit", "--literally", "junk"]);
    let broken = CommitId::from_hex(&broken_hex).expect("valid hex");

    // Structurally fine, but the committer timestamp is unparseable:
    // the walk must fall back instead of failing.
    std::fs::write(
        repo.path().join("weird"),
        format!(
            "tree {tree_hex}\nauthor A <a@a> notatime +9900\ncommitter C <c@c> notatime +9900\n\nweird time\n"
        ),
    )
    .expect("write weird");
    let weird_hex = repo.git(&["hash-object", "-w", "-t", "commit", "--literally", "weird"]);
    let weird = CommitId::from_hex(&weird_hex).expect("valid hex");

    let missing = CommitId::from_bytes([0x42; 20]);

    each_backend(repo.path(), |backend, name| {
        assert!(backend.meta(tree).is_err(), "{name}: tree is not a commit");
        assert!(
            backend.details(tree).is_err(),
            "{name}: tree has no details"
        );
        assert!(
            backend.meta(broken).is_err(),
            "{name}: broken commit meta errors"
        );
        assert!(
            backend.details(broken).is_err(),
            "{name}: broken commit details error"
        );
        assert!(
            backend.meta(missing).is_err(),
            "{name}: missing object errors"
        );

        // Unparseable timestamps must never panic; backends may either
        // fall back (gix reads the header leniently) or refuse the
        // object (libgit2 validates on load).
        if let Ok(meta) = backend.meta(weird) {
            assert_eq!(meta.time, 0, "{name}: unparseable time falls back to epoch");
        }
        if let Ok(details) = backend.details(weird) {
            assert_eq!(details.summary, "weird time", "{name}");
            assert_eq!(details.time, 0, "{name}");
        }

        assert!(
            matches!(
                backend.resolve(&tree_hex),
                Err(gixbi::Error::Resolve { .. })
            ),
            "{name}: a tree id is not a commitish"
        );
    });
}

#[test]
fn head_branch_reflects_repository_state() {
    let repo = TestRepo::new();
    each_backend(repo.path(), |backend, name| {
        assert_eq!(
            backend.head_branch(),
            None,
            "{name}: unborn HEAD decorates nothing"
        );
    });

    repo.commit("root");
    each_backend(repo.path(), |backend, name| {
        assert_eq!(backend.head_branch().as_deref(), Some("main"), "{name}");
        assert!(
            format!("{backend:?}").contains("git_dir"),
            "{name}: Debug names the git dir"
        );
    });

    repo.git(&["checkout", "-q", "--detach", "HEAD"]);
    each_backend(repo.path(), |backend, name| {
        assert_eq!(backend.head_branch(), None, "{name}: detached");
    });
}

#[test]
fn non_utf8_messages_render_lossily() {
    let repo = TestRepo::new();
    repo.commit("root");
    std::fs::write(repo.path().join("msg"), b"caf\xe9 latin1 subject\n\nbody\n")
        .expect("write msg");
    repo.git(&["commit", "--allow-empty", "-q", "-F", "msg"]);
    repo.git(&["branch", "other", "HEAD~1"]);

    for analysis in run(&repo, &["main", "other"]) {
        assert!(
            analysis.rows[0].summary.contains("latin1 subject"),
            "lossy decoding keeps the readable part"
        );
    }
}
