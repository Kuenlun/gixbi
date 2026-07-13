// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

//! Cell grid the graph is painted on.
//!
//! Every cell accumulates directional strokes (up/down/left/right) plus
//! an optional node marker; the final glyph is resolved from whichever
//! directions ended up present. Overlapping strokes therefore compose
//! into junctions (`├`, `┬`, `┼`, ...) instead of overwriting each
//! other, which is what makes the drawing collision-free by design.

/// One directional stroke: who draws it and whether it is elided.
#[derive(Debug, Clone, Copy)]
pub struct Stroke {
    /// Palette index of the branch the stroke belongs to.
    pub color: usize,
    /// Elided history (dashed) rather than a direct link.
    pub dashed: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, Default)]
struct Cell {
    node: Option<usize>,
    up: Option<Stroke>,
    down: Option<Stroke>,
    left: Option<Stroke>,
    right: Option<Stroke>,
}

/// What a resolved cell looks like, before theming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Blank,
    Node,
    Vertical {
        dashed: bool,
    },
    Horizontal {
        dashed: bool,
    },
    /// Corners, named by the directions they join.
    DownRight,
    DownLeft,
    UpRight,
    UpLeft,
    /// Junctions.
    VerticalRight,
    VerticalLeft,
    HorizontalUp,
    HorizontalDown,
    Cross,
}

/// A resolved cell: its shape and the colour that owns it.
#[derive(Debug, Clone, Copy)]
pub struct Resolved {
    pub shape: Shape,
    pub color: Option<usize>,
    pub is_node: bool,
}

/// Row-major grid of composable cells.
pub struct Canvas {
    width: usize,
    cells: Vec<Cell>,
}

impl Canvas {
    pub fn new(rows: usize, width: usize) -> Self {
        Self {
            width,
            cells: vec![Cell::default(); rows * width],
        }
    }

    pub const fn width(&self) -> usize {
        self.width
    }

    fn cell_mut(&mut self, row: usize, x: usize) -> &mut Cell {
        &mut self.cells[row * self.width + x]
    }

    /// Marks a commit node; the marker and its colour beat any stroke.
    pub fn node(&mut self, row: usize, x: usize, color: usize) {
        self.cell_mut(row, x).node = Some(color);
    }

    /// Adds one directional stroke; a solid stroke upgrades a dashed
    /// one, the first colour wins otherwise.
    pub fn stroke(&mut self, row: usize, x: usize, dir: Dir, stroke: Stroke) {
        let cell = self.cell_mut(row, x);
        let slot = match dir {
            Dir::Up => &mut cell.up,
            Dir::Down => &mut cell.down,
            Dir::Left => &mut cell.left,
            Dir::Right => &mut cell.right,
        };
        match slot {
            Some(existing) if existing.dashed && !stroke.dashed => *slot = Some(stroke),
            Some(_) => {}
            None => *slot = Some(stroke),
        }
    }

    /// Vertical run from `top` down to `bottom` (both node/corner rows):
    /// endpoints get the facing half, cells in between a full bar.
    pub fn vertical(&mut self, x: usize, top: usize, bottom: usize, stroke: Stroke) {
        self.stroke(top, x, Dir::Down, stroke);
        for row in top + 1..bottom {
            self.stroke(row, x, Dir::Up, stroke);
            self.stroke(row, x, Dir::Down, stroke);
        }
        self.stroke(bottom, x, Dir::Up, stroke);
    }

    /// Horizontal run between two x positions on one row, endpoints
    /// getting the facing half.
    pub fn horizontal(&mut self, row: usize, from: usize, to: usize, stroke: Stroke) {
        let (left, right) = if from <= to { (from, to) } else { (to, from) };
        self.stroke(row, left, Dir::Right, stroke);
        for x in left + 1..right {
            self.stroke(row, x, Dir::Left, stroke);
            self.stroke(row, x, Dir::Right, stroke);
        }
        self.stroke(row, right, Dir::Left, stroke);
    }

    /// Resolves one cell into a drawable shape and colour.
    pub fn resolve(&self, row: usize, x: usize) -> Resolved {
        let cell = self.cells[row * self.width + x];
        if let Some(color) = cell.node {
            return Resolved {
                shape: Shape::Node,
                color: Some(color),
                is_node: true,
            };
        }
        let (up, down, left, right) = (cell.up, cell.down, cell.left, cell.right);
        let shape = match (
            up.is_some(),
            down.is_some(),
            left.is_some(),
            right.is_some(),
        ) {
            (false, false, false, false) => Shape::Blank,
            (true, _, false, false) | (false, true, false, false) => Shape::Vertical {
                dashed: [up, down].iter().flatten().all(|stroke| stroke.dashed),
            },
            (false, false, true, _) | (false, false, false, true) => Shape::Horizontal {
                dashed: [left, right].iter().flatten().all(|stroke| stroke.dashed),
            },
            (false, true, false, true) => Shape::DownRight,
            (false, true, true, false) => Shape::DownLeft,
            (true, false, false, true) => Shape::UpRight,
            (true, false, true, false) => Shape::UpLeft,
            (true, true, false, true) => Shape::VerticalRight,
            (true, true, true, false) => Shape::VerticalLeft,
            (true, false, true, true) => Shape::HorizontalUp,
            (false, true, true, true) => Shape::HorizontalDown,
            (true, true, true, true) => Shape::Cross,
        };
        let color = [up, down, left, right]
            .iter()
            .flatten()
            .next()
            .map(|stroke| stroke.color);
        Resolved {
            shape,
            color,
            is_node: false,
        }
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    const SOLID: Stroke = Stroke {
        color: 3,
        dashed: false,
    };
    const DASHED: Stroke = Stroke {
        color: 5,
        dashed: true,
    };

    #[test]
    fn strokes_compose_into_junctions() {
        let mut canvas = Canvas::new(3, 3);
        canvas.vertical(1, 0, 2, SOLID);
        canvas.horizontal(1, 0, 2, SOLID);
        assert_eq!(canvas.resolve(1, 1).shape, Shape::Cross);
        assert_eq!(
            canvas.resolve(0, 1).shape,
            Shape::Vertical { dashed: false }
        );
        assert_eq!(
            canvas.resolve(1, 0).shape,
            Shape::Horizontal { dashed: false }
        );
        assert_eq!(canvas.resolve(0, 0).shape, Shape::Blank);
    }

    #[test]
    fn corners_and_tees_resolve_by_direction() {
        let mut canvas = Canvas::new(2, 5);
        // ╭─╮ over ╰─╯ plus a tee.
        canvas.stroke(0, 0, Dir::Down, SOLID);
        canvas.stroke(0, 0, Dir::Right, SOLID);
        canvas.stroke(0, 2, Dir::Down, SOLID);
        canvas.stroke(0, 2, Dir::Left, SOLID);
        canvas.stroke(1, 0, Dir::Up, SOLID);
        canvas.stroke(1, 0, Dir::Right, SOLID);
        canvas.stroke(1, 2, Dir::Up, SOLID);
        canvas.stroke(1, 2, Dir::Left, SOLID);
        canvas.stroke(0, 4, Dir::Down, SOLID);
        canvas.stroke(0, 4, Dir::Left, SOLID);
        canvas.stroke(0, 4, Dir::Right, SOLID);
        assert_eq!(canvas.resolve(0, 0).shape, Shape::DownRight);
        assert_eq!(canvas.resolve(0, 2).shape, Shape::DownLeft);
        assert_eq!(canvas.resolve(1, 0).shape, Shape::UpRight);
        assert_eq!(canvas.resolve(1, 2).shape, Shape::UpLeft);
        assert_eq!(canvas.resolve(0, 4).shape, Shape::HorizontalDown);

        let mut tees = Canvas::new(1, 3);
        for (x, dirs) in [
            (0, [Dir::Up, Dir::Down, Dir::Right].as_slice()),
            (1, [Dir::Up, Dir::Down, Dir::Left].as_slice()),
            (2, [Dir::Up, Dir::Left, Dir::Right].as_slice()),
        ] {
            for &dir in dirs {
                tees.stroke(0, x, dir, SOLID);
            }
        }
        assert_eq!(tees.resolve(0, 0).shape, Shape::VerticalRight);
        assert_eq!(tees.resolve(0, 1).shape, Shape::VerticalLeft);
        assert_eq!(tees.resolve(0, 2).shape, Shape::HorizontalUp);
    }

    #[test]
    fn node_beats_strokes_and_keeps_its_colour() {
        let mut canvas = Canvas::new(1, 1);
        canvas.vertical(0, 0, 0, SOLID);
        canvas.node(0, 0, 7);
        let resolved = canvas.resolve(0, 0);
        assert!(resolved.is_node);
        assert_eq!(resolved.shape, Shape::Node);
        assert_eq!(resolved.color, Some(7));
    }

    #[test]
    fn solid_upgrades_dashed_but_keeps_first_otherwise() {
        let mut canvas = Canvas::new(2, 1);
        canvas.vertical(0, 0, 1, DASHED);
        assert_eq!(canvas.resolve(0, 0).shape, Shape::Vertical { dashed: true });
        canvas.vertical(0, 0, 1, SOLID);
        let resolved = canvas.resolve(0, 0);
        assert_eq!(resolved.shape, Shape::Vertical { dashed: false });
        assert_eq!(resolved.color, Some(3), "solid stroke took over");
        canvas.vertical(0, 0, 1, DASHED);
        assert_eq!(
            canvas.resolve(0, 0).shape,
            Shape::Vertical { dashed: false }
        );
    }

    #[test]
    fn vertical_dashedness_needs_every_stroke_dashed() {
        let mut canvas = Canvas::new(3, 1);
        canvas.stroke(1, 0, Dir::Up, DASHED);
        canvas.stroke(1, 0, Dir::Down, SOLID);
        assert_eq!(
            canvas.resolve(1, 0).shape,
            Shape::Vertical { dashed: false }
        );
    }
}
