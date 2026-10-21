//! The OxTree binary: parses arguments, opens the window and hands the
//! state to eframe. `--bench PATH` scans and renders headless and prints
//! timings; `--screenshot FILE` saves the first full frame as PPM and
//! exits, for the change checklist.

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use eframe::egui;
use oxtree_ui::OxTreeApp;

struct Args {
    path: Option<PathBuf>,
    cache: Option<PathBuf>,
    bench: bool,
    screenshot: Option<PathBuf>,
    help: bool,
}

fn parse_args() -> Args {
    let mut args = Args {
        path: None,
        cache: None,
        bench: false,
        screenshot: None,
        help: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--bench" => args.bench = true,
            "--cache" => args.cache = it.next().map(PathBuf::from),
            "--screenshot" => args.screenshot = it.next().map(PathBuf::from),
            "-h" | "--help" => args.help = true,
            _ if a.starts_with("--") => args.help = true,
            _ => args.path = Some(PathBuf::from(a)),
        }
    }
    args
}

fn main() {
    let args = parse_args();
    if args.help {
        out(
            "usage: oxtree [DIRECTORY] [--cache FILE] [--bench] [--screenshot FILE.ppm]"
                .to_string(),
        );
        return;
    }
    if args.bench {
        bench(args.path.unwrap_or_else(|| PathBuf::from("/usr")));
        return;
    }
    let started = Instant::now();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OxTree")
            .with_app_id("oxtree")
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([640.0, 400.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "OxTree",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(Shell {
                app: OxTreeApp::new(args.path, args.cache, args.screenshot),
                started,
                first_frame: true,
            }))
        }),
    );
    if let Err(e) = result {
        eprintln!("OxTree could not open a window: {e}");
        std::process::exit(1);
    }
}

struct Shell {
    app: OxTreeApp,
    started: Instant,
    first_frame: bool,
}

impl eframe::App for Shell {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        if self.first_frame {
            self.first_frame = false;
            eprintln!(
                "first frame after {:.1} ms",
                self.started.elapsed().as_secs_f64() * 1000.0
            );
        }
        let app = &mut self.app;
        app.pump(&ctx);
        oxtree_ui::menus::handle_shortcuts(app, &ctx);
        oxtree_ui::tree_view::handle_keys(app, &ctx);

        egui::Panel::top("menu").show(root, |ui| oxtree_ui::menus::show(app, ui));
        egui::Panel::bottom("status").show(root, |ui| oxtree_ui::status_bar::show(app, ui));
        if app.show_details {
            egui::Panel::right("details")
                .resizable(true)
                .default_size(260.0)
                .show(root, |ui| {
                    egui::ScrollArea::vertical()
                        .show(ui, |ui| oxtree_ui::details_view::show(app, ui));
                });
        }
        if app.show_treemap {
            egui::Panel::bottom("treemap")
                .resizable(true)
                .default_size(300.0)
                .frame(egui::Frame::NONE)
                .show(root, |ui| oxtree_ui::treemap_view::show(app, ui));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(2.0))
            .show(root, |ui| oxtree_ui::tree_view::show(app, ui));

        oxtree_ui::dialogs::show(app, &ctx);
        screenshot_step(app, &ctx);
    }
}

/// Wait for the tree and the treemap texture, then a few frames, then ask
/// for a screenshot; write it as PPM and close.
fn screenshot_step(app: &mut OxTreeApp, ctx: &egui::Context) {
    let Some((path, countdown)) = app.screenshot.clone() else {
        return;
    };
    let ready = app.tree.is_some() && app.treemap.texture.is_some() && app.treemap.job.is_none();
    if ready {
        if countdown > 0 {
            app.screenshot = Some((path, countdown - 1));
            ctx.request_repaint();
            return;
        }
        if !app.quit_after_screenshot {
            app.quit_after_screenshot = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        // The captured frame arrives as an input event on a later frame, so
        // keep frames coming until it does.
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    } else {
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
        return;
    }
    let image = ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        })
    });
    if let Some(img) = image {
        let [w, h] = img.size;
        let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
        for px in &img.pixels {
            ppm.extend_from_slice(&[px.r(), px.g(), px.b()]);
        }
        match std::fs::write(&path, ppm) {
            Ok(()) => eprintln!("screenshot written to {}", path.display()),
            Err(e) => eprintln!("screenshot failed: {e}"),
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

/// Print without panicking when stdout is a closed pipe.
fn out(line: String) {
    let _ = writeln!(std::io::stdout(), "{line}");
}

fn bench(path: PathBuf) {
    let t0 = Instant::now();
    let (tree, scan_elapsed) =
        match oxtree_native::walker::scan_blocking(&path, oxtree_native::ScanOptions::default()) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        };
    let _ = t0;
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
}
