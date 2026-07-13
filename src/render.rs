// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

//! Terminal rendering of an [`Analysis`], in the visual language of
//! git-graph: one fixed colour-coded column per input branch, round
//! corners, and composable junctions.
//!
//! A dashed stroke always means "not a direct parent link": elided
//! commits within a branch line, or an interaction that travelled
//! through branches the graph does not show.

mod canvas;
mod layout;

use std::fmt::Write as _;

use anstyle::{AnsiColor, Color, Style};

use crate::analysis::{Analysis, Node};
use canvas::{Canvas, Shape, Stroke};
use layout::Route;

/// How to draw the graph.
#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    /// Emit ANSI colours.
    pub color: bool,
    /// Restrict the graph to ASCII glyphs.
    pub ascii: bool,
}

/// Renders the whole report: graph, truncation marker and per-pair
/// interaction summary.
#[must_use]
pub fn render(analysis: &Analysis, options: &RenderOptions) -> String {
    let theme = Theme {
        color: options.color,
        ascii: options.ascii,
    };
    let layout = layout::plan(analysis);
    let truncation_row = (analysis.truncated > 0).then_some(analysis.rows.len());
    let canvas = draw(analysis, &layout, truncation_row);

    let mut out = String::new();
    for (row, node) in analysis.rows.iter().enumerate() {
        let graph = graph_cells(&canvas, row, &theme);
        let text = row_text(analysis, node, &theme);
        let _ = writeln!(out, "{graph}  {text}");
    }
    if let Some(row) = truncation_row {
        let graph = graph_cells(&canvas, row, &theme);
        let noun = if analysis.truncated == 1 {
            "commit"
        } else {
            "commits"
        };
        let note = theme.paint(
            format_args!("{} older {noun} hidden by --limit", analysis.truncated),
            Theme::DIM,
        );
        let _ = writeln!(out, "{graph}  {note}");
    }
    let summary = summary_block(analysis, &theme);
    if !summary.is_empty() {
        let _ = write!(out, "\n{summary}");
    }
    out
}

/// Paints lines, edges and nodes onto a fresh canvas.
fn draw(analysis: &Analysis, layout: &layout::Layout, truncation_row: Option<usize>) -> Canvas {
    let rows = analysis.rows.len() + usize::from(truncation_row.is_some());
    let mut canvas = Canvas::new(rows, layout.width);

    for (branch, line) in analysis.lines.iter().enumerate() {
        let x = layout.column_x[branch];
        for (segment, pair) in line.rows.windows(2).enumerate() {
            canvas.vertical(x, pair[0], pair[1], stroke(branch, !line.adjacent[segment]));
        }
        if let (Some(&last), Some(fork)) = (line.rows.last(), line.fork) {
            let dashed = !line.adjacent[line.adjacent.len() - 1];
            canvas.vertical(x, last, fork, stroke(branch, dashed));
            canvas.horizontal(
                fork,
                x,
                layout.column_x[analysis.rows[fork].owner],
                stroke(branch, dashed),
            );
        }
        if let (Some(row), Some(&last), true) = (truncation_row, line.rows.last(), line.cut) {
            canvas.vertical(x, last, row, stroke(branch, true));
        }
    }

    for (edge, route) in analysis.edges.iter().zip(&layout.routes) {
        let stroke = stroke(edge.source_branch, !edge.direct);
        let source_x = layout.column_x[edge.source_branch];
        let target_x = layout.column_x[analysis.rows[edge.target].owner];
        let vertical_x = match route {
            Route::Ride => source_x,
            Route::Lane(x) => {
                canvas.horizontal(edge.source, source_x, *x, stroke);
                *x
            }
        };
        canvas.vertical(vertical_x, edge.target, edge.source, stroke);
        canvas.horizontal(edge.target, vertical_x, target_x, stroke);
    }

    for (row, node) in analysis.rows.iter().enumerate() {
        canvas.node(row, layout.column_x[node.owner], node.owner);
    }
    canvas
}

const fn stroke(branch: usize, dashed: bool) -> Stroke {
    Stroke {
        color: branch,
        dashed,
    }
}

/// One rendered graph row, padded to the full graph width.
fn graph_cells(canvas: &Canvas, row: usize, theme: &Theme) -> String {
    let mut out = String::new();
    for x in 0..canvas.width() {
        let resolved = canvas.resolve(row, x);
        let glyph = theme.glyph(resolved.shape);
        match resolved.color {
            Some(color) if resolved.shape != Shape::Blank => {
                out.push_str(&theme.paint(glyph, Theme::branch(color, resolved.is_node)));
            }
            _ => out.push(glyph),
        }
    }
    out
}

/// `hash date (decorations) summary  <- annotations`.
fn row_text(analysis: &Analysis, node: &Node, theme: &Theme) -> String {
    let mut text = String::new();
    let _ = write!(
        text,
        "{} {}",
        theme.paint(format_args!("{:.7}", node.id), Theme::HASH),
        theme.paint(date(node.time, node.offset), Theme::DIM),
    );
    if !node.tip_of.is_empty() {
        let decorations = node
            .tip_of
            .iter()
            .map(|&branch| {
                let info = &analysis.branches[branch];
                let name = theme.paint(&info.name, Theme::branch(branch, true));
                if info.is_head && info.name != "HEAD" {
                    format!("{} {name}", theme.paint("HEAD ->", Theme::HEAD))
                } else {
                    name
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let _ = write!(text, " ({decorations})");
    }
    if !node.summary.is_empty() {
        let _ = write!(text, " {}", node.summary);
    }
    for incoming in &node.incoming {
        let _ = write!(
            text,
            "  {} {} {}",
            theme.paint(
                format_args!(
                    "{} {}",
                    theme.arrow(),
                    analysis.branches[incoming.source_branch].name
                ),
                Theme::branch(incoming.source_branch, false),
            ),
            theme.paint("@", Theme::DIM),
            theme.paint(format_args!("{:.7}", incoming.source), Theme::HASH),
        );
        if !incoming.direct {
            let _ = write!(text, " {}", theme.paint("(indirect)", Theme::DIM));
        }
    }
    text
}

/// The `interactions:` block: latest event per ordered branch pair.
fn summary_block(analysis: &Analysis, theme: &Theme) -> String {
    if analysis.branches.len() < 2 {
        return String::new();
    }
    let label = |source: usize, target: usize| {
        (
            format!(
                "{} -> {}",
                analysis.branches[source].name, analysis.branches[target].name
            ),
            format!(
                "{} -> {}",
                theme.paint(
                    &analysis.branches[source].name,
                    Theme::branch(source, false)
                ),
                theme.paint(
                    &analysis.branches[target].name,
                    Theme::branch(target, false)
                ),
            ),
        )
    };
    let width = analysis
        .summaries
        .iter()
        .map(|pair| label(pair.source, pair.target).0.len())
        .max()
        .unwrap_or(0);

    let mut out = String::from("interactions:\n");
    for pair in &analysis.summaries {
        let (plain, coloured) = label(pair.source, pair.target);
        let pad = " ".repeat(width - plain.len());
        match &pair.hit {
            Some(hit) => {
                let _ = write!(
                    out,
                    "  {coloured}{pad}  at {} {}, up to {}",
                    theme.paint(format_args!("{:.7}", hit.merge), Theme::HASH),
                    theme.paint(date(hit.time, hit.offset), Theme::DIM),
                    theme.paint(format_args!("{:.7}", hit.source), Theme::HASH),
                );
                if !hit.direct {
                    let _ = write!(out, " {}", theme.paint("(indirect)", Theme::DIM));
                }
                out.push('\n');
            }
            None => {
                let _ = writeln!(
                    out,
                    "  {coloured}{pad}  {}",
                    theme.paint("never", Theme::DIM)
                );
            }
        }
    }
    out
}

/// `YYYY-MM-DD` of a commit in its own recorded timezone.
fn date(time: i64, offset: i32) -> String {
    // Howard Hinnant's civil-from-days algorithm.
    let days = (time + i64::from(offset)).div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Palette and glyph tables; every colour decision funnels through here.
struct Theme {
    color: bool,
    ascii: bool,
}

const PALETTE: [AnsiColor; 12] = [
    AnsiColor::Blue,
    AnsiColor::Green,
    AnsiColor::Magenta,
    AnsiColor::Cyan,
    AnsiColor::Red,
    AnsiColor::Yellow,
    AnsiColor::BrightBlue,
    AnsiColor::BrightGreen,
    AnsiColor::BrightMagenta,
    AnsiColor::BrightCyan,
    AnsiColor::BrightRed,
    AnsiColor::BrightYellow,
];

impl Theme {
    const HASH: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow)));
    const DIM: Style = Style::new().dimmed();
    const HEAD: Style = Style::new()
        .fg_color(Some(Color::Ansi(AnsiColor::Cyan)))
        .bold();

    const fn branch(index: usize, bold: bool) -> Style {
        let style = Style::new().fg_color(Some(Color::Ansi(PALETTE[index % PALETTE.len()])));
        if bold { style.bold() } else { style }
    }

    fn paint(&self, text: impl std::fmt::Display, style: Style) -> String {
        if self.color {
            format!("{}{text}{}", style.render(), style.render_reset())
        } else {
            text.to_string()
        }
    }

    const fn arrow(&self) -> &'static str {
        if self.ascii { "<-" } else { "\u{2190}" }
    }

    const fn glyph(&self, shape: Shape) -> char {
        match shape {
            Shape::Blank => ' ',
            Shape::Node => {
                if self.ascii {
                    '*'
                } else {
                    '\u{25cf}' // ●
                }
            }
            Shape::Vertical { dashed } => match (self.ascii, dashed) {
                (true, false) => '|',
                (true, true) => ':',
                (false, true) => '\u{2506}',  // ┆
                (false, false) => '\u{2502}', // │
            },
            Shape::Horizontal { dashed } => match (self.ascii, dashed) {
                (true, false) => '-',
                (true, true) => '~',
                (false, true) => '\u{2504}',  // ┄
                (false, false) => '\u{2500}', // ─
            },
            Shape::DownRight => {
                if self.ascii {
                    '.'
                } else {
                    '\u{256d}' // ╭
                }
            }
            Shape::DownLeft => {
                if self.ascii {
                    '.'
                } else {
                    '\u{256e}' // ╮
                }
            }
            Shape::UpRight => {
                if self.ascii {
                    '\''
                } else {
                    '\u{2570}' // ╰
                }
            }
            Shape::UpLeft => {
                if self.ascii {
                    '\''
                } else {
                    '\u{256f}' // ╯
                }
            }
            Shape::VerticalRight => {
                if self.ascii {
                    '+'
                } else {
                    '\u{251c}' // ├
                }
            }
            Shape::VerticalLeft => {
                if self.ascii {
                    '+'
                } else {
                    '\u{2524}' // ┤
                }
            }
            Shape::HorizontalUp => {
                if self.ascii {
                    '+'
                } else {
                    '\u{2534}' // ┴
                }
            }
            Shape::HorizontalDown => {
                if self.ascii {
                    '+'
                } else {
                    '\u{252c}' // ┬
                }
            }
            Shape::Cross => {
                if self.ascii {
                    '+'
                } else {
                    '\u{253c}' // ┼
                }
            }
        }
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::analysis::{Options, analyze};
    use crate::testutil::MemRepo;

    fn plain() -> RenderOptions {
        RenderOptions::default()
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    fn run(
        repo: &MemRepo,
        branches: &[&str],
        analysis_options: &Options,
        render_options: &RenderOptions,
    ) -> String {
        let analysis = analyze(repo, &names(branches), analysis_options).unwrap();
        render(&analysis, render_options)
    }

    /// main: 1 <- 2 <- 4 (merge of feature's 3, forked at 2).
    fn merged_repo() -> MemRepo {
        MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit_named(3, &[2], 30, "feature work")
            .commit_named(4, &[2, 3], 40, "merge feature")
            .branch("main", 4)
            .branch("feature", 3)
    }

    #[test]
    fn merge_renders_the_classic_shape() {
        let out = run(
            &merged_repo(),
            &["main", "feature"],
            &Options::default(),
            &plain(),
        );
        let expected = "\
\u{25cf}\u{2500}\u{256e}  0404040 1970-01-01 (main) merge feature  \u{2190} feature @ 0303030
\u{2502} \u{25cf}  0303030 1970-01-01 (feature) feature work
\u{25cf}\u{2500}\u{256f}  0202020 1970-01-01 commit 0x02

interactions:
  main -> feature  never
  feature -> main  at 0404040 1970-01-01, up to 0303030
";
        assert_eq!(out, expected);
    }

    #[test]
    fn octopus_composes_tees_on_both_rows() {
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit(3, &[1], 21)
            .commit(9, &[1, 2, 3], 40)
            .branch("main", 9)
            .branch("a", 2)
            .branch("b", 3);
        let out = run(&repo, &["main", "a", "b"], &Options::default(), &plain());
        let graph: Vec<&str> = out
            .lines()
            .take(4)
            .map(|line| line.split("  ").next().unwrap())
            .collect();
        assert_eq!(
            graph,
            [
                "\u{25cf}\u{2500}\u{252c}\u{2500}\u{256e}",
                "\u{2502} \u{2502} \u{25cf}",
                "\u{2502} \u{25cf} \u{2502}",
                "\u{25cf}\u{2500}\u{2534}\u{2500}\u{256f}",
            ],
            "octopus merge joins and forks compose into tees"
        );
    }

    #[test]
    fn criss_cross_forces_a_lane() {
        // a: 1 <- a2 <- am (merges b2); b: 1 <- b2 <- bm (merges a2).
        // bm sits between am and its source b2, so that edge gets a
        // lane while the other edge rides a's column.
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(0x0a, &[1], 20)
            .commit(0x0b, &[1], 25)
            .commit(0xaa, &[0x0a, 0x0b], 50)
            .commit(0xbb, &[0x0b, 0x0a], 40)
            .branch("a", 0xaa)
            .branch("b", 0xbb);
        let out = run(&repo, &["a", "b"], &Options::default(), &plain());
        let graph: Vec<String> = out
            .lines()
            .take(5)
            .map(|line| {
                line.chars()
                    .take(5)
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect();
        assert_eq!(
            graph,
            [
                "\u{25cf}\u{2500}\u{256e}",
                "\u{251c}\u{2500}\u{253c}\u{2500}\u{25cf}",
                "\u{2502} \u{2570}\u{2500}\u{25cf}",
                "\u{25cf}   \u{2502}",
                "\u{25cf}\u{2500}\u{2500}\u{2500}\u{256f}",
            ],
            "lane keeps the b2->am edge clear of bm's row"
        );
    }

    #[test]
    fn ascii_theme_swaps_every_glyph() {
        let out = run(
            &merged_repo(),
            &["main", "feature"],
            &Options::default(),
            &RenderOptions {
                color: false,
                ascii: true,
            },
        );
        assert!(out.starts_with("*-."), "ascii merge corner");
        assert!(out.contains("*-'"), "ascii fork corner");
        assert!(out.contains("<- feature"), "ascii arrow");
        assert!(!out.contains('\u{25cf}'), "no unicode leaks through");
    }

    #[test]
    fn colors_emit_ansi_sequences() {
        let out = run(
            &merged_repo(),
            &["main", "feature"],
            &Options::default(),
            &RenderOptions {
                color: true,
                ascii: false,
            },
        );
        assert!(out.contains("\u{1b}["), "ANSI colours requested");
        let plain_out = run(
            &merged_repo(),
            &["main", "feature"],
            &Options::default(),
            &plain(),
        );
        assert!(!plain_out.contains("\u{1b}["), "no ANSI without color");
    }

    #[test]
    fn truncation_row_marks_open_lines() {
        let repo = merged_repo();
        let options = Options {
            all: false,
            limit: Some(2),
        };
        let out = run(&repo, &["main", "feature"], &options, &plain());
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines[1],
            "\u{2506} \u{25cf}  0303030 1970-01-01 (feature) feature work"
        );
        assert_eq!(
            lines[2],
            "\u{2506} \u{2506}  1 older commit hidden by --limit"
        );
    }

    #[test]
    fn elided_history_renders_dashed() {
        // main: 1..4 linear with an extra commit between fork and tip;
        // feature merged at the tip, so commit 3 is hidden in sparse mode.
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit(3, &[2], 30)
            .commit(6, &[2], 25)
            .commit(4, &[3, 6], 40)
            .branch("main", 4)
            .branch("feature", 6);
        let out = run(&repo, &["main", "feature"], &Options::default(), &plain());
        let lines: Vec<&str> = out.lines().collect();
        // Between the merge row and the fork row, main's own line skips
        // commit 3: the vertical must be dashed.
        assert_eq!(
            lines[0].split("  ").next().unwrap(),
            "\u{25cf}\u{2500}\u{256e}"
        );
        assert_eq!(lines[1].split("  ").next().unwrap(), "\u{2506} \u{25cf}");
        assert_eq!(
            lines[2].split("  ").next().unwrap(),
            "\u{25cf}\u{2500}\u{256f}"
        );
    }

    #[test]
    fn indirect_edges_render_dashed_and_annotated() {
        // X: 1 <- 2 <- 3; hidden branch 5 <- 6 off X at 2; Y merges 6.
        let repo = MemRepo::new()
            .commit(1, &[], 10)
            .commit(2, &[1], 20)
            .commit(3, &[2], 30)
            .commit(4, &[1], 25)
            .commit(5, &[2], 35)
            .commit(6, &[5], 45)
            .commit(7, &[4, 6], 50)
            .branch("x", 3)
            .branch("y", 7);
        let out = run(&repo, &["x", "y"], &Options::default(), &plain());
        assert!(out.contains("(indirect)"));
        assert!(
            out.contains('\u{2504}'),
            "indirect edge horizontal is dashed"
        );
        assert!(
            out.contains("up to 0202020 (indirect)"),
            "summary marks indirection"
        );
    }

    #[test]
    fn single_branch_renders_without_summary() {
        let repo = MemRepo::new().commit(1, &[], 10).branch("solo", 1);
        let out = run(&repo, &["solo"], &Options::default(), &plain());
        assert_eq!(out, "\u{25cf}  0101010 1970-01-01 (solo) commit 0x01\n");
    }

    #[test]
    fn zero_limit_renders_only_the_truncation_note() {
        let options = Options {
            all: false,
            limit: Some(0),
        };
        let out = run(&merged_repo(), &["main", "feature"], &options, &plain());
        assert_eq!(out.lines().count(), 5, "truncation note plus summary");
        assert!(
            out.trim_start()
                .starts_with("3 older commits hidden by --limit")
        );
    }

    #[test]
    fn head_decoration_prefixes_the_branch_name() {
        let repo = merged_repo().head("main");
        let out = run(&repo, &["main", "feature"], &Options::default(), &plain());
        assert!(out.contains("(HEAD -> main)"));
    }

    #[test]
    fn dates_follow_the_recorded_offset() {
        assert_eq!(date(0, 0), "1970-01-01");
        assert_eq!(date(0, -1), "1969-12-31");
        assert_eq!(date(86_399, 0), "1970-01-01");
        assert_eq!(date(86_399, 1), "1970-01-02");
        assert_eq!(date(951_782_400, 0), "2000-02-29", "leap day");
        assert_eq!(date(1_767_225_599, 0), "2025-12-31");
        assert_eq!(
            date(1_767_225_600, 3600),
            "2026-01-01",
            "offset crosses new year"
        );
        assert_eq!(date(-86_400, 0), "1969-12-31", "pre-epoch");
    }
}
