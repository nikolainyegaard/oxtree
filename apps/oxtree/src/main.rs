//! The OxTree binary: parses arguments, opens the window and hands the
//! state to eframe. `--bench PATH` scans and renders headless and prints
//! timings; `--screenshot FILE` saves the first full frame as PPM and
//! exits, for the change checklist.

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use eframe::egui;
use oxtree_ui::OxTreeApp;

#[cfg(feature = "diagnostics")]
mod diagnostics;

/// Without the feature the diagnostics are not compiled at all; these stubs
/// keep the frame loop free of conditionals.
#[cfg(not(feature = "diagnostics"))]
mod diagnostics {
    pub fn startup_mark(_label: &str) {}
    pub fn first_frame(_started: std::time::Instant) {}
    pub fn install_logger() {}
    pub fn configure(_app: &mut oxtree_ui::OxTreeApp, screenshot: Option<std::path::PathBuf>) {
        if screenshot.is_some() {
            eprintln!("--screenshot needs a build with --features diagnostics");
        }
    }
    pub fn screenshot_step(_app: &mut oxtree_ui::OxTreeApp, _ctx: &eframe::egui::Context) {}
}

use diagnostics::{screenshot_step, startup_mark};

struct Args {
    path: Option<PathBuf>,
    cache: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    files_view: bool,
    types_view: bool,
    search: Option<String>,
    /// `--export FILE`: scan, write CSV, exit; `-` is stdout.
    export: Option<PathBuf>,
    tsv: bool,
    utc: bool,
    max_depth: Option<u32>,
    percent_of_parent: bool,
    help: bool,
}

fn parse_args() -> Args {
    let mut args = Args {
        path: None,
        cache: None,
        screenshot: None,
        files_view: false,
        types_view: false,
        search: None,
        export: None,
        tsv: false,
        utc: false,
        max_depth: None,
        percent_of_parent: false,
        help: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--cache" => args.cache = it.next().map(PathBuf::from),
            "--screenshot" => args.screenshot = it.next().map(PathBuf::from),
            "--files" => args.files_view = true,
            "--types" => args.types_view = true,
            "--search" => args.search = it.next(),
            "--export" => args.export = it.next().map(PathBuf::from),
            "--tsv" => args.tsv = true,
            "--utc" => args.utc = true,
            "--max-depth" => args.max_depth = it.next().and_then(|v| v.parse().ok()),
            "--percent" => args.percent_of_parent = true,
            "-h" | "--help" => args.help = true,
            _ if a.starts_with("--") => args.help = true,
            _ => args.path = Some(PathBuf::from(a)),
        }
    }
    args
}

fn main() {
    startup_mark("main entered: exec, dynamic loader, static init");
    diagnostics::install_logger();
    let args = parse_args();
    if args.help {
        out(
            "usage: oxtree [DIRECTORY] [--cache FILE] [--files] [--types] [--search QUERY]"
                .to_string(),
        );
        out("       oxtree DIRECTORY --export FILE|- [--tsv] [--utc] [--max-depth N] [--percent] [--search QUERY]".to_string());
        return;
    }
    if let Some(file) = &args.export {
        std::process::exit(export(&args, file));
    }
    let started = Instant::now();
    startup_mark("arguments parsed");
    let settings = oxtree_native::config::load_settings();
    let early = oxtree_ui::app::begin_early(
        if args.cache.is_none() {
            args.path.clone()
        } else {
            None
        },
        settings.clone(),
    );
    startup_mark("settings read, config and date +%z started on a thread");
    let (w, h) = settings.window.unwrap_or((1200.0, 800.0));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(if oxtree_native::root::is_root() {
                "OxTree [root]"
            } else {
                "OxTree"
            })
            // The desktop file's name, so Wayland compositors find the icon.
            .with_app_id("com.github.nikolainyegaard.oxtree")
            .with_icon(egui::IconData {
                rgba: include_bytes!("../../../packaging/oxtree-64.rgba").to_vec(),
                width: 64,
                height: 64,
            })
            .with_inner_size([w, h])
            .with_min_inner_size([640.0, 400.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "OxTree",
        options,
        Box::new(move |cc| {
            startup_mark("window, GL context and egui ready (winit, Wayland, glutin, Mesa)");
            cc.egui_ctx.set_fonts(fonts());
            Ok(Box::new(Shell {
                app: {
                    let mut app = OxTreeApp::new(early, args.cache);
                    app.attach_context(&cc.egui_ctx);
                    diagnostics::configure(&mut app, args.screenshot);
                    startup_mark("OxTreeApp::new: joined the config thread, categories");
                    if args.files_view {
                        app.view = oxtree_ui::app::View::Files;
                    }
                    if args.types_view {
                        app.view = oxtree_ui::app::View::Types;
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
                tree_seen: false,
            }))
        }),
    );
    if let Err(e) = result {
        eprintln!("OxTree could not open a window: {e}");
        std::process::exit(1);
    }
}

/// Headless export: scan the directory (or read the cache file) and write
/// WizTree's CSV columns, filtered by `--search` when given. Returns the
/// exit code.
fn export(args: &Args, file: &PathBuf) -> i32 {
    let tree = match (&args.cache, &args.path) {
        (Some(c), _) => oxtree_native::cache_io::read(c),
        (None, Some(p)) => {
            let settings = oxtree_native::config::load_settings();
            oxtree_native::walker::scan_blocking(
                p,
                oxtree_native::ScanOptions {
                    threads: oxtree_ui::app::scan_threads(),
                    cross_filesystems: settings.cross_filesystems,
                    mounts: oxtree_native::mounts::MountTable::load(),
                    ..Default::default()
                },
            )
            .map(|(t, _)| t)
        }
        (None, None) => Err("--export needs a directory or --cache FILE".into()),
    };
    let tree = match tree {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let utc_offset = if args.utc {
        0
    } else {
        oxtree_native::localtime::utc_offset_secs()
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let query = match args.search.as_deref() {
        Some(q) => match oxtree_core::query::Query::parse(q, now, utc_offset) {
            Ok(q) => Some(q),
            Err(e) => {
                eprintln!("--search: {e}");
                return 1;
            }
        },
        None => None,
    };
    let opts = oxtree_core::export::ExportOptions {
        separator: if args.tsv { '\t' } else { ',' },
        max_depth: args.max_depth,
        utc_offset,
        percent_of_parent: args.percent_of_parent,
    };
    let text = oxtree_core::export::csv(&tree, tree.root(), &opts, |n| {
        let Some(q) = &query else { return true };
        let path = tree.path(n);
        q.matches(
            &oxtree_core::query::Item {
                name: tree.name(n),
                path: &path,
                size: tree.size(n),
                mtime: tree.mtime(n),
                is_dir: tree.is_dir_like(n),
            },
            true,
        )
    });
    let result = if file.as_os_str() == "-" {
        std::io::stdout().write_all(text.as_bytes())
    } else {
        std::fs::write(file, text)
    };
    match result {
        Ok(()) => 0,
        // A reader that stopped early, `head` for one, is not an error.
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => 0,
        Err(e) => {
            eprintln!("Cannot write {}: {e}", file.display());
            1
        }
    }
}

/// One bundled face, Ubuntu Light, for both families, plus egui's small
/// icon font for the glyphs its widgets draw. Hack, egui's monospace face,
/// is left out: the two monospace labels in the app read fine in Ubuntu
/// and the font is 309 KB. Scripts the bundled face lacks, CJK and emoji
/// in file names among them, come from the system's fonts on demand
/// through eframe's system font provider.
fn fonts() -> egui::FontDefinitions {
    use egui::{FontData, FontFamily, FontTweak};
    use std::sync::Arc;
    let mut defs = egui::FontDefinitions::empty();
    defs.font_data.insert(
        "Ubuntu-Light".to_owned(),
        Arc::new(FontData::from_static(epaint_default_fonts::UBUNTU_LIGHT)),
    );
    defs.font_data.insert(
        "egui-icons".to_owned(),
        Arc::new(
            FontData::from_static(epaint_default_fonts::EGUI_ICONS).tweak(FontTweak {
                scale: 0.90,
                ..Default::default()
            }),
        ),
    );
    let stack = vec!["Ubuntu-Light".to_owned(), "egui-icons".to_owned()];
    defs.families
        .insert(FontFamily::Proportional, stack.clone());
    defs.families.insert(FontFamily::Monospace, stack);
    defs
}

struct Shell {
    app: OxTreeApp,
    started: Instant,
    first_frame: bool,
    frames: u32,
    tree_seen: bool,
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

    /// The settings file is written once, here; the Preferences dialog
    /// writes it too when it closes.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.app.save_settings();
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let screen = ctx.content_rect();
        self.app.window_size = Some((screen.width(), screen.height()));
        if self.first_frame {
            self.first_frame = false;
            startup_mark("first update called: event loop warm-up, first redraw request");
            diagnostics::first_frame(self.started);
            oxtree_native::root::signal_ready();
        }
        let app = &mut self.app;
        app.pump(&ctx);
        if !self.tree_seen && app.tree.is_some() {
            self.tree_seen = true;
            startup_mark(&format!(
                "tree installed: the scan itself took {:.0} ms; the first frame with content follows",
                app.last_elapsed.map(|e| e.as_secs_f64() * 1000.0).unwrap_or(0.0)
            ));
        }
        oxtree_ui::menus::handle_shortcuts(app, &ctx);
        match app.view {
            oxtree_ui::app::View::Tree => oxtree_ui::tree_view::handle_keys(app, &ctx),
            oxtree_ui::app::View::Files => oxtree_ui::file_view::handle_keys(app, &ctx),
            oxtree_ui::app::View::Types => {}
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
                    oxtree_ui::app::View::Types => oxtree_ui::types_view::show(app, ui),
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

/// Print without panicking when stdout is a closed pipe.
fn out(line: String) {
    let _ = writeln!(std::io::stdout(), "{line}");
}
