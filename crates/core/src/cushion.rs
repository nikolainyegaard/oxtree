//! Per-pixel cushion shading of laid-out tiles into one RGBA buffer, split
//! into horizontal bands across threads. Directories get QDirStat's default
//! diagonal gradient; files get the cushion with their category colour.

use crate::layout::Tile;
use crate::tree::{NodeId, Tree};

#[derive(Clone, Copy, Debug)]
pub struct Shading {
    pub dir_gradient_start: [u8; 3],
    pub dir_gradient_end: [u8; 3],
    /// 0 to 255 like QDirStat's `AmbientLight` setting.
    pub ambient_light: u8,
    pub light: [f64; 3],
}

impl Default for Shading {
    fn default() -> Self {
        Shading {
            dir_gradient_start: [0x60, 0x60, 0x70],
            dir_gradient_end: [0x70, 0x70, 0x80],
            ambient_light: 40,
            light: [-0.09759, -0.19518, 0.9759],
        }
    }
}

/// Render `tiles` into a `w` x `h` RGBA8 buffer. `color_of` gives a file
/// tile's base colour. Rows are split into `threads` bands.
pub fn render(
    tree: &Tree,
    tiles: &[Tile],
    w: usize,
    h: usize,
    color_of: &(dyn Fn(NodeId) -> [u8; 3] + Sync),
    shading: Shading,
    threads: usize,
) -> Vec<u8> {
    let mut buf = vec![0u8; w * h * 4];
    if w == 0 || h == 0 {
        return buf;
    }
    let threads = threads.clamp(1, h);
    let rows_per_band = h.div_ceil(threads);
    let band_bytes = rows_per_band * w * 4;
    std::thread::scope(|scope| {
        for (band_index, band) in buf.chunks_mut(band_bytes).enumerate() {
            let y0 = band_index * rows_per_band;
            scope.spawn(move || render_band(tree, tiles, w, y0, band, color_of, shading));
        }
    });
    buf
}

fn render_band(
    tree: &Tree,
    tiles: &[Tile],
    w: usize,
    y0: usize,
    band: &mut [u8],
    color_of: &(dyn Fn(NodeId) -> [u8; 3] + Sync),
    shading: Shading,
) {
    let rows = band.len() / (w * 4);
    let y1 = y0 + rows;
    let ia = shading.ambient_light as f64 / 255.0;
    let is = 1.0 - ia;
    let (lx, ly, lz) = (
        is * shading.light[0],
        is * shading.light[1],
        is * shading.light[2],
    );

    for t in tiles {
        let r = t.rect;
        let tx0 = r.x.max(0.0) as usize;
        let tx1 = ((r.x + r.w) as usize).min(w);
        let ty0 = (r.y.max(0.0) as usize).max(y0);
        let ty1 = ((r.y + r.h) as usize).min(y1);
        if tx1 <= tx0 || ty1 <= ty0 {
            continue;
        }
        if tree.is_dir_like(t.node) {
            let denom = (r.w * r.w + r.h * r.h).max(1.0);
            let (a, b) = (shading.dir_gradient_start, shading.dir_gradient_end);
            for y in ty0..ty1 {
                let Some(row) =
                    band.get_mut((y - y0) * w * 4 + tx0 * 4..(y - y0) * w * 4 + tx1 * 4)
                else {
                    continue;
                };
                let dy = (y as f64 - r.y) * r.h;
                for (i, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let f = ((tx0 + i) as f64 - r.x) * r.w + dy;
                    let f = (f / denom).clamp(0.0, 1.0);
                    write_px(
                        px,
                        [
                            lerp(a[0], b[0], f),
                            lerp(a[1], b[1], f),
                            lerp(a[2], b[2], f),
                        ],
                    );
                }
            }
        } else {
            let color = color_of(t.node);
            let xx22 = t.cushion.xx2 * 2.0;
            let yy22 = t.cushion.yy2 * 2.0;
            for y in ty0..ty1 {
                let Some(row) =
                    band.get_mut((y - y0) * w * 4 + tx0 * 4..(y - y0) * w * 4 + tx1 * 4)
                else {
                    continue;
                };
                let ny = yy22 * (y as f64 + 0.5) + t.cushion.yy1;
                for (i, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let nx = xx22 * ((tx0 + i) as f64 + 0.5) + t.cushion.xx1;
                    let mut cosa = (lz - ny * ly - nx * lx) / (nx * nx + ny * ny + 1.0).sqrt();
                    if cosa < 0.0 {
                        cosa = 0.0;
                    }
                    cosa += ia;
                    write_px(
                        px,
                        [
                            shade(color[0], cosa),
                            shade(color[1], cosa),
                            shade(color[2], cosa),
                        ],
                    );
                }
            }
        }
    }
}

fn lerp(a: u8, b: u8, f: f64) -> u8 {
    (a as f64 + (b as f64 - a as f64) * f) as u8
}

fn shade(c: u8, cosa: f64) -> u8 {
    (cosa * c as f64 + 0.5).min(255.0) as u8
}

fn write_px(px: &mut [u8; 4], rgb: [u8; 3]) {
    *px = [rgb[0], rgb[1], rgb[2], 255];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{layout, Rect, DEFAULT_MIN_TILE_SIZE};
    use crate::tree::{DirRecord, FileRecord, Kind};

    #[test]
    fn every_pixel_is_painted_and_bands_agree_with_single_thread() {
        let files: Vec<FileRecord> = (0..20)
            .map(|i| FileRecord {
                name: format!("f{i}"),
                size: 1000 * (20 - i),
                mtime: 0,
                mode: 0o100644,
                kind: Kind::File,
            })
            .collect();
        let dirs = vec![DirRecord {
            parent: None,
            name: "r".into(),
            own_size: 0,
            kind: Some(Kind::Dir),
            files,
            subdirs: vec![],
            ..Default::default()
        }];
        let t = Tree::build("/r".into(), dirs).unwrap();
        let tiles = layout(
            &t,
            0,
            Rect::new(0.0, 0.0, 64.0, 48.0),
            DEFAULT_MIN_TILE_SIZE,
        );
        let color = |_: NodeId| [200u8, 100, 50];
        let one = render(&t, &tiles, 64, 48, &color, Shading::default(), 1);
        let many = render(&t, &tiles, 64, 48, &color, Shading::default(), 5);
        assert_eq!(one, many);
        assert!(one.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        // Shaded pixels are never brighter than the base colour plus ambient light.
        assert!(one
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[1] <= 160 && p[2] <= 90));
    }
}
