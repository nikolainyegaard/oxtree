//! Squarified treemap layout with cushion surfaces, following QDirStat's
//! `TreemapTile.cpp` (Bruls, Huizing and van Wijk's squarify, van Wijk and
//! van de Wetering's cushions). Children are already sorted largest first
//! in the tree, so this is a traversal with arithmetic and no sorting.

use crate::tree::{NodeId, Tree};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }

    pub fn area(&self) -> f64 {
        self.w * self.h
    }
}

/// Polynomial coefficients of a tile's cushion surface. Each ridge added
/// by an ancestor's layout row or the tile's own rectangle steepens it.
#[derive(Clone, Copy, Default, Debug)]
pub struct Cushion {
    pub xx2: f64,
    pub xx1: f64,
    pub yy2: f64,
    pub yy1: f64,
    ridges: u32,
}

impl Cushion {
    /// Regressive factors QDirStat found by experiment in 11/2023: a constant
    /// 4.0 was far too dark for deep trees, a small constant too flat for
    /// shallow ones.
    fn coefficient(&self) -> f64 {
        match self.ridges {
            0..=2 => 1.5,
            3 | 4 => 1.3,
            5..=7 => 1.2,
            _ => 1.1,
        }
    }

    fn add_ridge(&mut self, horizontal: bool, r: Rect) {
        self.ridges += 1;
        let c = self.coefficient();
        let (x1, x2) = if horizontal {
            (r.x as i64, (r.x + r.w) as i64)
        } else {
            (r.y as i64, (r.y + r.h) as i64)
        };
        if x1 == x2 {
            return;
        }
        let span = (x2 - x1) as f64;
        let sum = (x2 + x1) as f64;
        if horizontal {
            self.xx2 -= c / span;
            self.xx1 += c * sum / span;
        } else {
            self.yy2 -= c / span;
            self.yy1 += c * sum / span;
        }
    }
}

#[derive(Clone, Debug)]
pub struct Tile {
    pub node: NodeId,
    pub rect: Rect,
    pub cushion: Cushion,
}

/// Tiles whose longer side is below this many pixels are not laid out.
pub const DEFAULT_MIN_TILE_SIZE: f64 = 3.0;

/// Lay out the subtree under `root` into `rect`. The first tile is the root
/// itself; every tile comes after its parent, so the deepest tile that
/// contains a point is the last match when searching backwards.
pub fn layout(tree: &Tree, root: NodeId, rect: Rect, min_tile_size: f64) -> Vec<Tile> {
    let mut tiles = Vec::new();
    tiles.push(Tile {
        node: root,
        rect,
        cushion: Cushion::default(),
    });
    let mut scratch = Vec::new();
    squarified_children(
        tree,
        root,
        rect,
        Cushion::default(),
        min_tile_size,
        &mut tiles,
        &mut scratch,
    );
    tiles
}

/// The deepest tile containing the point.
pub fn hit_test(tiles: &[Tile], x: f64, y: f64) -> Option<&Tile> {
    tiles.iter().rev().find(|t| t.rect.contains(x, y))
}

/// The tile for a node, if it was laid out.
pub fn tile_of(tiles: &[Tile], node: NodeId) -> Option<&Tile> {
    tiles.iter().find(|t| t.node == node)
}

#[allow(clippy::too_many_arguments)]
fn squarified_children(
    tree: &Tree,
    parent: NodeId,
    rect: Rect,
    cushion: Cushion,
    min_tile: f64,
    tiles: &mut Vec<Tile>,
    scratch: &mut Vec<(NodeId, u64)>,
) {
    let total = tree.size(parent);
    if total == 0 || rect.w < 1.0 || rect.h < 1.0 {
        return;
    }
    let scale = rect.w * rect.h / total as f64;
    let min_size = (min_tile / scale) as u64;

    // Children are sorted largest first, so the first one below the minimum
    // ends the useful part of the block.
    let start = scratch.len();
    for c in tree.children(parent) {
        let s = tree.size(c);
        if s < min_size {
            break;
        }
        scratch.push((c, s));
    }
    let count = scratch.len() - start;
    if count == 0 {
        return;
    }
    let mut remaining: u64 = if min_size > 0 {
        total
    } else {
        scratch
            .get(start..)
            .map(|s| s.iter().map(|e| e.1).fold(0, u64::saturating_add))
            .unwrap_or(0)
    };

    let mut rect = rect;
    let mut i = start;
    let end = scratch.len();
    while i < end {
        let row_end = squarify(scratch, i, end, rect, remaining);
        let row: Vec<(NodeId, u64)> = scratch
            .get(i..row_end)
            .map(|r| r.to_vec())
            .unwrap_or_default();
        rect = layout_row(
            tree, &row, rect, remaining, cushion, min_tile, tiles, scratch,
        );
        for &(_, s) in &row {
            remaining = remaining.saturating_sub(s);
        }
        i = row_end;
    }
    scratch.truncate(start);
}

fn squarify(
    entries: &[(NodeId, u64)],
    start: usize,
    end: usize,
    rect: Rect,
    remaining: u64,
) -> usize {
    let length = rect.w.min(rect.h);
    let height_total = rect.w.max(rect.h);
    if length == 0.0 || height_total == 0.0 || remaining == 0 {
        return (start + 1).min(end);
    }
    let first = entries.get(start).map(|e| e.1).unwrap_or(0) as f64;
    let first_scale = first * length;
    let mut best = 0.0f64;
    let mut sum = 0.0f64;
    let mut i = start;
    while i < end {
        let size = entries.get(i).map(|e| e.1).unwrap_or(0) as f64;
        sum += size;
        if size != 0.0 && sum != 0.0 {
            let height = height_total * sum / remaining as f64;
            let first_width = first_scale / sum;
            let last_width = length * size / sum;
            if height == 0.0 || first_width == 0.0 {
                break;
            }
            let aspect = (height / first_width).min(last_width / height);
            if aspect < best {
                break;
            }
            best = aspect;
        }
        i += 1;
    }
    i.max(start + 1).min(end)
}

#[allow(clippy::too_many_arguments)]
fn layout_row(
    tree: &Tree,
    row: &[(NodeId, u64)],
    rect: Rect,
    remaining: u64,
    cushion: Cushion,
    min_tile: f64,
    tiles: &mut Vec<Tile>,
    scratch: &mut Vec<(NodeId, u64)>,
) -> Rect {
    if row.is_empty() || remaining == 0 {
        return rect;
    }
    let horizontal = rect.w < rect.h;
    let primary = rect.w.min(rect.h).floor();
    let sum: u64 = row.iter().map(|e| e.1).fold(0, u64::saturating_add);
    if sum == 0 {
        return rect;
    }
    let secondary = (sum as f64 * rect.w.max(rect.h) / remaining as f64 + 0.5).floor();
    if secondary < min_tile {
        return rect;
    }

    let mut row_cushion = cushion;
    if horizontal {
        row_cushion.add_ridge(false, Rect::new(rect.x, rect.y, primary, secondary));
    } else {
        row_cushion.add_ridge(true, Rect::new(rect.x, rect.y, secondary, primary));
    }

    let mut offset = 0.0f64;
    let mut left = primary;
    for &(node, size) in row {
        let mut child_size = size as f64 / sum as f64 * primary;
        if child_size > left {
            child_size = left;
        }
        left -= child_size;
        if child_size >= min_tile {
            let r = if horizontal {
                Rect::new(
                    rect.x + offset.round(),
                    rect.y,
                    child_size.ceil(),
                    secondary,
                )
            } else {
                Rect::new(
                    rect.x,
                    rect.y + offset.round(),
                    secondary,
                    child_size.ceil(),
                )
            };
            let mut tc = row_cushion;
            tc.add_ridge(horizontal, r);
            tiles.push(Tile {
                node,
                rect: r,
                cushion: tc,
            });
            if tree.is_dir_like(node) {
                squarified_children(tree, node, r, tc, min_tile, tiles, scratch);
            }
            offset += child_size;
        }
    }
    if horizontal {
        Rect::new(rect.x, rect.y + secondary, rect.w, rect.h - secondary)
    } else {
        Rect::new(rect.x + secondary, rect.y, rect.w - secondary, rect.h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{DirRecord, FileRecord, Kind};

    fn tree() -> Tree {
        let files: Vec<FileRecord> = (0..50)
            .map(|i| FileRecord {
                name: format!("f{i}"),
                size: 1000 * (50 - i),
                mtime: 0,
                mode: 0o100644,
                kind: Kind::File,
            })
            .collect();
        let dirs = vec![
            DirRecord {
                parent: None,
                name: "r".into(),
                own_size: 0,
                kind: Some(Kind::Dir),
                files: files.clone(),
                subdirs: vec![1],
                ..Default::default()
            },
            DirRecord {
                parent: Some(0),
                name: "s".into(),
                own_size: 0,
                kind: Some(Kind::Dir),
                files,
                subdirs: vec![],
                ..Default::default()
            },
        ];
        Tree::build("/r".into(), dirs).unwrap()
    }

    #[test]
    fn tiles_stay_inside_their_parent_and_siblings_do_not_overlap() {
        let t = tree();
        let tiles = layout(
            &t,
            0,
            Rect::new(0.0, 0.0, 800.0, 400.0),
            DEFAULT_MIN_TILE_SIZE,
        );
        assert!(tiles.len() > 50);
        for tile in &tiles {
            let r = tile.rect;
            assert!(
                r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= 801.0 && r.y + r.h <= 401.0,
                "{r:?}"
            );
            assert!(r.w >= 1.0 && r.h >= 1.0);
        }
        let s = t.children(0).next().unwrap();
        let kids: Vec<&Tile> = tiles
            .iter()
            .filter(|x| t.parent(x.node) == Some(s))
            .collect();
        for (i, a) in kids.iter().enumerate() {
            for b in kids.iter().skip(i + 1) {
                let overlap_w =
                    (a.rect.x + a.rect.w).min(b.rect.x + b.rect.w) - a.rect.x.max(b.rect.x);
                let overlap_h =
                    (a.rect.y + a.rect.h).min(b.rect.y + b.rect.h) - a.rect.y.max(b.rect.y);
                assert!(
                    overlap_w <= 1.0 || overlap_h <= 1.0,
                    "{:?} overlaps {:?}",
                    a.rect,
                    b.rect
                );
            }
        }
    }

    #[test]
    fn hit_test_returns_the_deepest_tile() {
        let t = tree();
        let tiles = layout(
            &t,
            0,
            Rect::new(0.0, 0.0, 800.0, 400.0),
            DEFAULT_MIN_TILE_SIZE,
        );
        let biggest_file = tiles
            .iter()
            .filter(|x| !t.is_dir_like(x.node))
            .max_by(|a, b| a.rect.area().partial_cmp(&b.rect.area()).unwrap())
            .unwrap();
        let hit = hit_test(&tiles, biggest_file.rect.x + 1.0, biggest_file.rect.y + 1.0).unwrap();
        assert_eq!(hit.node, biggest_file.node);
        assert!(hit_test(&tiles, 5000.0, 5000.0).is_none());
    }

    #[test]
    fn tiny_rect_gives_only_the_root() {
        let t = tree();
        let tiles = layout(&t, 0, Rect::new(0.0, 0.0, 2.0, 2.0), DEFAULT_MIN_TILE_SIZE);
        assert_eq!(tiles.len(), 1);
    }
}
