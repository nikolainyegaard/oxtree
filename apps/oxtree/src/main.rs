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
    files_view: bool,
    search: Option<String>,
    help: bool,
}

fn parse_args() -> Args {
    let mut args = Args {
        path: None,
        cache: None,
        bench: false,
        screenshot: None,
        files_view: false,
        search: None,
        help: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--bench" => args.bench = true,
            "--cache" => args.cache = it.next().map(PathBuf::from),
            "--screenshot" => args.screenshot = it.next().map(PathBuf::from),
            "--files" => args.files_view = true,
            "--search" => args.search = it.next(),
            "-h" | "--help" => args.help = true,
            _ if a.starts_with("--") => args.help = true,
            _ => args.path = Some(PathBuf::from(a)),
        }
    }
    args
}

/// `OXTREE_STARTUP=1` prints where the time before the first frame goes.
/// `OXTREE_T0` may carry the launching shell's epoch nanoseconds so the
/// exec and loader stage is measured too.
fn startup_mark(label: &str) {
    thread_local! {
        static LAST: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
    }
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !*ON.get_or_init(|| std::env::var_os("OXTREE_STARTUP").is_some()) {
        return;
    }
    LAST.with(|last| {
        let now = Instant::now();
        let since = last.get().map(|l| (now - l).as_secs_f64() * 1000.0);
        last.set(Some(now));
        match since {
            Some(ms) => eprintln!("startup  +{ms:7.2} ms  {label}"),
            None => {
                let from_shell = std::env::var("OXTREE_T0")
                    .ok()
                    .and_then(|t| t.parse::<u128>().ok())
                    .and_then(|t0| {
                        let now_ns = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .ok()?
                            .as_nanos();
                        Some((now_ns.saturating_sub(t0)) as f64 / 1e6)
                    });
                match from_shell {
                    Some(ms) => eprintln!("startup  +{ms:7.2} ms  {label} (from the shell's exec)"),
                    None => eprintln!("startup           {label}"),
                }
            }
        }
    });
}

/// With `OXTREE_STARTUP=1`, every log record from eframe, winit, glutin,
/// the Wayland client and egui is printed with the time since the last
/// one, which is the only way to see where the window setup spends its
/// time without a tracer.
struct StartupLogger {
    last: std::sync::Mutex<Option<Instant>>,
}

impl log::Log for StartupLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        let now = Instant::now();
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        let since = last
            .map(|l| (now - l).as_secs_f64() * 1000.0)
            .unwrap_or(0.0);
        *last = Some(now);
        eprintln!(
            "startup  +{since:7.2} ms    [{}] {}",
            record.target(),
            record.args()
        );
    }
    fn flush(&self) {}
}

static STARTUP_LOGGER: StartupLogger = StartupLogger {
    last: std::sync::Mutex::new(None),
};

fn main() {
    startup_mark("main entered: exec, dynamic loader, static init");
    if std::env::var_os("OXTREE_STARTUP").is_some() {
        let _ = log::set_logger(&STARTUP_LOGGER);
        log::set_max_level(log::LevelFilter::Trace);
    }
    let args = parse_args();
    if args.help {
        out(
            "usage: oxtree [DIRECTORY] [--cache FILE] [--files] [--search QUERY] [--bench] [--screenshot FILE.ppm]"
                .to_string(),
        );
        return;
    }
    if args.bench {
        bench(args.path.unwrap_or_else(|| PathBuf::from("/usr")));
        return;
    }
    let started = Instant::now();
    startup_mark("arguments parsed");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(if oxtree_native::root::is_root() {
                "OxTree [root]"
            } else {
                "OxTree"
            })
            .with_app_id("oxtree")
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([640.0, 400.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "OxTree",
        options,
        Box::new(move |_cc| {
            startup_mark("window, GL context and egui ready (winit, Wayland, glutin, Mesa)");
            Ok(Box::new(Shell {
                app: {
                    let mut app = OxTreeApp::new(args.path, args.cache, args.screenshot);
                    startup_mark("OxTreeApp::new: config, date +%z, categories, scan thread start");
                    if args.files_view {
                        app.view = oxtree_ui::app::View::Files;
                    }
                    if let Some(s) = args.search {
                        app.view = oxtree_ui::app::View::Files;
                        app.file_view.search = s;
                    }
                    app
                },
                started,
                first_frame: true,
                frames: 0,
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
    frames: u32,
}

impl eframe::App for Shell {
    /// eframe's default clear colour has an alpha of 180, and the window
    /// surface is declared opaque, so on Wayland every pixel the views do not
    /// paint themselves blended with whatever the compositor had there
    /// before: stale frames, the wallpaper, the window at its old position.
    /// The tree area and the treemap panel have no frame fill, so that was a
    /// third of the window. An opaque clear colour ends it.
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.extreme_bg_color.to_normalized_gamma_f32()
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        if self.first_frame {
            self.first_frame = false;
            startup_mark("first update called: event loop warm-up, first redraw request");
            eprintln!(
                "first frame after {:.1} ms",
                self.started.elapsed().as_secs_f64() * 1000.0
            );
            oxtree_native::root::signal_ready();
        }
        let app = &mut self.app;
        app.pump(&ctx);
        oxtree_ui::menus::handle_shortcuts(app, &ctx);
        match app.view {
            oxtree_ui::app::View::Tree => oxtree_ui::tree_view::handle_keys(app, &ctx),
            oxtree_ui::app::View::Files => oxtree_ui::file_view::handle_keys(app, &ctx),
        }

        egui::Panel::top("menu").show(root, |ui| oxtree_ui::menus::show(app, ui));
        egui::Panel::bottom("status").show(root, |ui| oxtree_ui::status_bar::show(app, ui));
        if app.show_details {
            egui::Panel::right("details")
                .resizable(true)
                .min_size(260.0)
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
            .show(root, |ui| {
                oxtree_ui::file_view::tabs(app, ui);
                match app.view {
                    oxtree_ui::app::View::Tree => oxtree_ui::tree_view::show(app, ui),
                    oxtree_ui::app::View::Files => oxtree_ui::file_view::show(app, ui),
                }
            });

        oxtree_ui::dialogs::show(app, &ctx);
        screenshot_step(app, &ctx);
        if self.frames < 3 {
            self.frames += 1;
            match self.frames {
                1 => startup_mark("first update built: menus, panels, views, font atlas"),
                2 => startup_mark("second update: the first frame was tessellated, uploaded, drawn and swapped before this"),
                _ => {}
            }
        }
    }
}

/// Wait for the tree and the treemap texture, then a few frames, then ask
/// for a screenshot; write it as PPM and close.
fn screenshot_step(app: &mut OxTreeApp, ctx: &egui::Context) {
    let Some((path, countdown)) = app.screenshot.clone() else {
        return;
    };
    let ready = app.tree.is_some() && app.treemap.texture.is_some() && app.treemap.job.is_none();
    if !ready {
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
        return;
    }
    if countdown > 0 {
        app.screenshot = Some((path, countdown - 1));
        ctx.request_repaint();
        return;
    }
    if app.quit_after_screenshot {
        return;
    }
    app.quit_after_screenshot = true;
    // The capture runs after this frame is painted; the callback writes the
    // file and closes the window.
    let ctx2 = ctx.clone();
    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
        egui::ScreenshotCallback::new(move |img| {
            let [w, h] = img.size;
            let min_alpha = img.pixels.iter().map(|p| p.a()).min().unwrap_or(0);
            let translucent = img.pixels.iter().filter(|p| p.a() < 255).count();
            eprintln!(
                "screenshot alpha: min {min_alpha}, {translucent} of {} pixels below 255",
                img.pixels.len()
            );
            let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
            for px in &img.pixels {
                ppm.extend_from_slice(&[px.r(), px.g(), px.b()]);
            }
            match std::fs::write(&path, ppm) {
                Ok(()) => eprintln!("screenshot written to {}", path.display()),
                Err(e) => eprintln!("screenshot failed: {e}"),
            }
            ctx2.send_viewport_cmd(egui::ViewportCommand::Close);
        }),
    ));
}

/// Print without panicking when stdout is a closed pipe.
fn out(line: String) {
    let _ = writeln!(std::io::stdout(), "{line}");
}

fn bench(path: PathBuf) {
    let t0 = Instant::now();
    let (tree, scan_elapsed) =
        match oxtree_native::walker::scan_blocking(&path, bench_scan_options()) {
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

/// The benchmark honours the same environment switch as the app, so a scan
/// of / can be timed with and without crossing filesystems.
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
