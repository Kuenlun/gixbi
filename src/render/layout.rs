// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

//! Horizontal geometry: every branch gets a fixed column (command-line
//! order) and every interaction edge gets a collision-free vertical
//! path.
//!
//! An edge normally *rides* its source branch column: the vertical run
//! up to the merge row overlaps the branch's own line, which is exactly
//! the "this line flows into that commit" reading. When another commit
//! of that branch sits between the two rows, riding would visually
//! attach the edge to the wrong commit, so the edge gets a transient
//! *lane*: an extra column inserted next to the source, packed so that
//! edges only share a lane when their row ranges do not overlap.

use crate::analysis::Analysis;

/// Vertical path of one edge, aligned with `Analysis::edges`.
#[derive(Debug, PartialEq, Eq)]
pub enum Route {
    /// Ride the source branch column.
    Ride,
    /// Use a dedicated lane at this x position.
    Lane(usize),
}

/// Planned horizontal geometry for one graph.
pub struct Layout {
    /// Total width of the graph area, in cells.
    pub width: usize,
    /// X position of every branch column.
    pub column_x: Vec<usize>,
    /// Vertical path of every edge.
    pub routes: Vec<Route>,
}

enum Raw {
    Ride,
    Lane { anchor: usize, slot: usize },
}

pub fn plan(analysis: &Analysis) -> Layout {
    let branches = analysis.lines.len();
    // Lane slots between column c and c+1: per slot, the (top, bottom)
    // row intervals already reserved.
    let mut lanes_after: Vec<Vec<Vec<(usize, usize)>>> = vec![Vec::new(); branches];

    let raw: Vec<Raw> = analysis
        .edges
        .iter()
        .map(|edge| {
            let source_col = edge.source_branch;
            let target_col = analysis.rows[edge.target].owner;
            let blocked =
                ((edge.target + 1)..edge.source).any(|row| analysis.rows[row].owner == source_col);
            if blocked {
                // Lane next to the source column, on the target's side.
                let anchor = if target_col > source_col {
                    source_col
                } else {
                    source_col - 1
                };
                let slot = reserve(&mut lanes_after[anchor], edge.target, edge.source);
                Raw::Lane { anchor, slot }
            } else {
                Raw::Ride
            }
        })
        .collect();

    let mut column_x = Vec::with_capacity(branches);
    let mut x = 0;
    for lanes in &lanes_after {
        column_x.push(x);
        x += 2 + 2 * lanes.len();
    }
    let width = x.saturating_sub(1);
    let routes = raw
        .into_iter()
        .map(|route| match route {
            Raw::Ride => Route::Ride,
            Raw::Lane { anchor, slot } => Route::Lane(column_x[anchor] + 2 * (slot + 1)),
        })
        .collect();
    Layout {
        width,
        column_x,
        routes,
    }
}

/// First lane slot whose reserved intervals do not overlap [top, bottom].
fn reserve(lanes: &mut Vec<Vec<(usize, usize)>>, top: usize, bottom: usize) -> usize {
    let slot = lanes
        .iter()
        .position(|taken| taken.iter().all(|&(t, b)| bottom < t || b < top))
        .unwrap_or_else(|| {
            lanes.push(Vec::new());
            lanes.len() - 1
        });
    lanes[slot].push((top, bottom));
    slot
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::analysis::{Analysis, BranchInfo, BranchLine, Edge, Node};
    use crate::repo::CommitId;

    fn node(owner: usize) -> Node {
        Node {
            id: CommitId::from_bytes([u8::try_from(owner).unwrap() + 1; 20]),
            owner,
            time: 0,
            offset: 0,
            summary: String::new(),
            tip_of: Vec::new(),
            incoming: Vec::new(),
        }
    }

    fn skeleton(owners: &[usize], edges: Vec<Edge>) -> Analysis {
        let branches = owners.iter().max().map_or(0, |&max| max + 1);
        Analysis {
            branches: (0..branches)
                .map(|_| BranchInfo {
                    name: String::new(),
                    tip: None,
                    is_head: false,
                })
                .collect(),
            rows: owners.iter().map(|&owner| node(owner)).collect(),
            lines: (0..branches)
                .map(|_| BranchLine {
                    rows: Vec::new(),
                    fork: None,
                    adjacent: Vec::new(),
                    cut: false,
                })
                .collect(),
            edges,
            summaries: Vec::new(),
            truncated: 0,
        }
    }

    fn edge(source: usize, target: usize, source_branch: usize) -> Edge {
        Edge {
            source,
            target,
            source_branch,
            direct: true,
        }
    }

    #[test]
    fn unblocked_edges_ride_their_source_column() {
        let analysis = skeleton(&[0, 1, 0], vec![edge(1, 0, 1)]);
        let layout = plan(&analysis);
        assert_eq!(layout.routes, [Route::Ride]);
        assert_eq!(layout.column_x, [0, 2]);
        assert_eq!(layout.width, 3);
    }

    #[test]
    fn blocked_edges_get_a_lane_between_the_columns() {
        // Row 1 belongs to branch 0 and sits between source and target.
        let analysis = skeleton(&[1, 0, 0], vec![edge(2, 0, 0)]);
        let layout = plan(&analysis);
        assert_eq!(layout.routes, [Route::Lane(2)]);
        assert_eq!(layout.column_x, [0, 4]);
        assert_eq!(layout.width, 5);
    }

    #[test]
    fn lane_toward_a_left_target_sits_left_of_the_source() {
        // Source branch 1, target branch 0 with a branch-1 row between.
        let analysis = skeleton(&[0, 1, 1, 1], vec![edge(3, 0, 1)]);
        let layout = plan(&analysis);
        assert_eq!(layout.routes, [Route::Lane(2)]);
        assert_eq!(layout.column_x, [0, 4]);
    }

    #[test]
    fn overlapping_lanes_stack_disjoint_ones_share() {
        let analysis = skeleton(
            &[1, 0, 0, 0, 1, 0, 0],
            vec![
                edge(2, 0, 0), // rows 0..2
                edge(3, 0, 0), // rows 0..3, overlaps -> second slot
                edge(6, 4, 0), // rows 4..6, disjoint from the first -> reuses slot 0
            ],
        );
        let layout = plan(&analysis);
        assert_eq!(
            layout.routes,
            [Route::Lane(2), Route::Lane(4), Route::Lane(2)]
        );
        assert_eq!(layout.column_x, [0, 6]);
        assert_eq!(layout.width, 7);
    }
}
