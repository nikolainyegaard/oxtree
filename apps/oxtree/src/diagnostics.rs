//! Development diagnostics, compiled only with `--features diagnostics`:
//! the startup timing switch and the screenshot mode. The benchmark is the
//! separate `oxtree-bench` binary. None of this is in the shipped build.

use std::io::Write;
use std::time::Instant;

use eframe::egui;
use oxtree_ui::OxTreeApp;

/// `OXTREE_STARTUP` prints where the time before the first frame goes:
/// `1` the stages only, about ten lines; `2` the stages plus every log
/// record from eframe, winit, glutin and the Wayland client that followed
/// a gap of a quarter millisecond or more; a file path, everything, written
/// to that file. `OXTREE_T0` may carry the launching shell's epoch
/// nanoseconds so the exec and loader stage is measured too.
struct Startup {
    last: std::sync::Mutex<Option<Instant>>,
    sink: std::sync::Mutex<Option<std::fs::File>>,
    /// 0 off, 1 stages, 2 stages and slow records, 3 everything.
    level: u8,
}

static STARTUP: std::sync::OnceLock<Startup> = std::sync::OnceLock::new();

fn startup() -> &'static Startup {
    STARTUP.get_or_init(|| {
        let value = std::env::var("OXTREE_STARTUP").unwrap_or_default();
        let (level, sink) = match value.as_str() {
            "" => (0, None),
            "1" => (1, None),
            "2" => (2, None),
            path if path.starts_with('/') => (3, std::fs::File::create(path).ok()),
            _ => (3, None),
        };
        Startup {
            last: std::sync::Mutex::new(None),
            sink: std::sync::Mutex::new(sink),
            level,
        }
    })
}

impl Startup {
    /// Milliseconds since the previous line, and the line itself.
    fn emit(&self, since: Option<f64>, text: &str) {
        let line = match since {
            Some(ms) => format!("startup  +{ms:7.2} ms  {text}"),
            None => format!("startup           {text}"),
        };
        let mut sink = self.sink.lock().unwrap_or_else(|e| e.into_inner());
        match sink.as_mut() {
            Some(f) => {
                let _ = writeln!(f, "{line}");
            }
            None => eprintln!("{line}"),
        }
    }

    fn since_last(&self) -> Option<f64> {
        let now = Instant::now();
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        let since = last.map(|l| (now - l).as_secs_f64() * 1000.0);
        *last = Some(now);
        since
    }
}

pub fn startup_mark(label: &str) {
    let st = startup();
    if st.level == 0 {
        return;
    }
    let since = st.since_last();
    if since.is_some() {
        st.emit(since, label);
        return;
    }
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
        Some(ms) => st.emit(Some(ms), &format!("{label} (from the shell's exec)")),
        None => st.emit(None, label),
    }
}

struct StartupLogger;

impl log::Log for StartupLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        startup().level >= 2
    }
    fn log(&self, record: &log::Record) {
        let st = startup();
        if st.level < 2 {
            return;
        }
        let since = st.since_last().unwrap_or(0.0);
        if st.level == 2 && since < 0.25 {
            return;
        }
        st.emit(
            Some(since),
            &format!("  [{}] {}", record.target(), record.args()),
        );
    }
    fn flush(&self) {}
}

static STARTUP_LOGGER: StartupLogger = StartupLogger;

/// Route eframe's, winit's and glutin's log records through the startup
/// logger when `OXTREE_STARTUP` asks for them.
pub fn install_logger() {
    if startup().level >= 2 {
        let _ = log::set_logger(&STARTUP_LOGGER);
        log::set_max_level(log::LevelFilter::Trace);
    }
}

/// `--screenshot FILE.ppm`: arm the app to write its first full frame.
pub fn configure(app: &mut OxTreeApp, screenshot: Option<std::path::PathBuf>) {
    if let Some(p) = screenshot {
        app.set_screenshot(p);
    }
}

/// Wait for the tree and the treemap texture, then a few frames, then ask
/// for a screenshot; write it as PPM and close.
pub fn screenshot_step(app: &mut OxTreeApp, ctx: &egui::Context) {
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

/// The one line the performance budget is measured with: time from `main`
/// to the first frame, on stderr.
pub fn first_frame(started: std::time::Instant) {
    eprintln!(
        "first frame after {:.1} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
}
