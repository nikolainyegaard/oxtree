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
    search: Option<String>,
    help: bool,
}

fn parse_args() -> Args {
    let mut args = Args {
        path: None,
        cache: None,
        screenshot: None,
        files_view: false,
        search: None,
        help: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
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

fn main() {
    startup_mark("main entered: exec, dynamic loader, static init");
    diagnostics::install_logger();
    let args = parse_args();
    if args.help {
        out("usage: oxtree [DIRECTORY] [--cache FILE] [--files] [--search QUERY]".to_string());
        return;
    }
    let started = Instant::now();
    startup_mark("arguments parsed");
    let early = oxtree_ui::app::begin_early(if args.cache.is_none() {
        args.path.clone()
    } else {
        None
    });
    startup_mark("config and date +%z started on a thread");
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
        Box::new(move |cc| {
            startup_mark("window, GL context and egui ready (winit, Wayland, glutin, Mesa)");
            Ok(Box::new(Shell {
                app: {
                    let mut app = OxTreeApp::new(early, args.cache);
                    app.attach_context(&cc.egui_ctx);
                    diagnostics::configure(&mut app, args.screenshot);
                    startup_mark("OxTreeApp::new: joined the config thread, categories");
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
                tree_seen: false,
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

/// Print without panicking when stdout is a closed pipe.
fn out(line: String) {
    let _ = writeln!(std::io::stdout(), "{line}");
}
