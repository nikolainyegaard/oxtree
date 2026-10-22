//! `oxtree-bench PATH`: scan, layout, render, cache and search timings on
//! one tree, printed to stdout. Built only with `--features diagnostics`.

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr"));
    bench(path);
}

/// Print without panicking when stdout is a closed pipe.
fn out(line: String) {
    let _ = writeln!(std::io::stdout(), "{line}");
}

fn bench(path: PathBuf) {
    let (tree, scan_elapsed) =
        match oxtree_native::walker::scan_blocking(&path, bench_scan_options()) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        };
    out(format!(
        "scan     {:>8.1} ms  {} nodes, {}",
        scan_elapsed.as_secs_f64() * 1000.0,
        tree.len(),
        oxtree_core::format::size(tree.size(0))
    ));
    let categorizer = oxtree_core::mime::Categorizer::with_defaults();
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(16);
    for (w, h) in [(1900usize, 600usize), (1893, 597)] {
        let t1 = Instant::now();
        let tiles = oxtree_core::layout::layout(
            &tree,
            0,
            oxtree_core::layout::Rect::new(0.0, 0.0, w as f64, h as f64),
            3.0,
        );
        let t2 = Instant::now();
        let color_of = |n: u32| {
            categorizer
                .category(tree.name(n), tree.kind(n), tree.mode(n))
                .map(|c| c.color)
                .unwrap_or([0xde, 0x8d, 0x53])
        };
        let px = oxtree_core::cushion::render(
            &tree,
            &tiles,
            w,
            h,
            &color_of,
            Default::default(),
            threads,
        );
        let t3 = Instant::now();
        out(format!(
            "layout   {:>8.1} ms  {} tiles at {w}x{h}",
            (t2 - t1).as_secs_f64() * 1000.0,
            tiles.len()
        ));
        out(format!(
            "render   {:>8.1} ms  {} threads",
            (t3 - t2).as_secs_f64() * 1000.0,
            threads
        ));
        if w == 1900 {
            let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
            for p in px.as_chunks::<4>().0 {
                ppm.extend_from_slice(&p[..3]);
            }
            let _ = std::fs::write("treemap.ppm", ppm);
        }
    }
    let t4 = Instant::now();
    let text = oxtree_core::cache::write(&tree);
    let t5 = Instant::now();
    let back = oxtree_core::cache::parse(&text).and_then(|(r, d)| oxtree_core::Tree::build(r, d));
    let t6 = Instant::now();
    out(format!(
        "cache    {:>8.1} ms write, {:>8.1} ms parse, {} bytes, ok={}",
        (t5 - t4).as_secs_f64() * 1000.0,
        (t6 - t5).as_secs_f64() * 1000.0,
        text.len(),
        back.is_ok()
    ));
    let t = Instant::now();
    let types = oxtree_core::types::file_types(&tree, &categorizer);
    out(format!(
        "types    {:>8.1} ms  {} extensions",
        t.elapsed().as_secs_f64() * 1000.0,
        types.len()
    ));
    let t = Instant::now();
    let dupes = oxtree_core::dupes::duplicate_counts(&tree, oxtree_core::dupes::DupMode::NameSize);
    out(format!(
        "dupes    {:>8.1} ms  {} files with a duplicate by name and size",
        t.elapsed().as_secs_f64() * 1000.0,
        dupes.iter().filter(|&&c| c > 1).count()
    ));
    let t = Instant::now();
    let csv = oxtree_core::export::csv(&tree, 0, &Default::default(), |_| true);
    out(format!(
        "csv      {:>8.1} ms  {} bytes",
        t.elapsed().as_secs_f64() * 1000.0,
        csv.len()
    ));
    // Search: a name query, then the same query against whole paths, which
    // builds a path string per node.
    for (label, match_path) in [("name", false), ("path", true)] {
        let q = oxtree_core::query::Query::parse("lib >1k", 0, 0).unwrap_or_default();
        let needs_path = q.needs_path(match_path);
        let t = Instant::now();
        let hits = oxtree_core::flat::flat_list_filtered(
            &tree,
            false,
            oxtree_core::flat::FlatSort::Size,
            false,
            0,
            |n| {
                let path = if needs_path {
                    tree.path(n)
                } else {
                    String::new()
                };
                q.matches(
                    &oxtree_core::query::Item {
                        name: tree.name(n),
                        path: &path,
                        size: tree.size(n),
                        mtime: tree.mtime(n),
                        is_dir: tree.is_dir_like(n),
                    },
                    match_path,
                )
            },
        )
        .len();
        out(format!(
            "search   {:>8.1} ms  \"lib >1k\" by {label}, {hits} hits",
            t.elapsed().as_secs_f64() * 1000.0
        ));
    }
}

/// `OXTREE_CROSS_FILESYSTEMS=0` turns crossing off for a timing run; the
/// app itself takes this from the settings file.
fn bench_scan_options() -> oxtree_native::ScanOptions {
    let cross =
        !std::env::var_os("OXTREE_CROSS_FILESYSTEMS").is_some_and(|v| v == "0" || v == "false");
    oxtree_native::ScanOptions {
        cross_filesystems: cross,
        mounts: if cross {
            oxtree_native::mounts::MountTable::load()
        } else {
            Default::default()
        },
        ..Default::default()
    }
}
