//! Running as root, and replacing this instance with one running as root
//! through polkit (`osascript` with administrator privileges on macOS).
//! Unix has no way to raise a running process's privileges, so "the full
//! picture" is another process: `pkexec oxtree PATH`, as QDirStat
//! documents with sudo or pkexec. Windows instances are always elevated. The root instance gets
//! the invoking user's display and config directory through the
//! environment and never writes configuration, so nothing ends up owned by
//! root. Once it has drawn its first frame it touches a ready file, and the
//! instance that started it closes, so the user sees one window hand over
//! to the next rather than two.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};

/// The effective user id from /proc/self/status, without libc; from `id
/// -u` on macOS. `None` on Windows.
pub fn effective_uid() -> Option<u32> {
    if cfg!(windows) {
        return None;
    }
    if cfg!(target_os = "macos") {
        let out = Command::new("id").arg("-u").output().ok()?;
        return String::from_utf8_lossy(&out.stdout).trim().parse().ok();
    }
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("Uid:"))?;
    line.split_whitespace().nth(2)?.parse().ok()
}

/// Root on Unix. On Windows the manifest makes every instance run as
/// administrator, which the MFT reader needs, so this is always true.
pub fn is_root() -> bool {
    cfg!(windows) || effective_uid() == Some(0)
}

/// What the second instance reported: its exit code once it closed, or why
/// it could not start. pkexec exits 126 when the dialog is dismissed and
/// 127 when authorization is refused.
pub type LaunchResult = Result<i32, String>;

/// A root instance on its way: its exit comes on `exit`, and `ready_file`
/// appears once it has a window.
pub struct Launch {
    pub exit: Receiver<LaunchResult>,
    pub ready_file: PathBuf,
}

impl Launch {
    /// Whether the root instance has drawn its first frame. The file is
    /// removed on the first true.
    pub fn is_ready(&self) -> bool {
        if self.ready_file.exists() {
            let _ = std::fs::remove_file(&self.ready_file);
            true
        } else {
            false
        }
    }
}

/// Called by the instance that was started as root, after its first frame.
pub fn signal_ready() {
    if let Some(p) = std::env::var_os("OXTREE_READY_FILE") {
        let _ = std::fs::write(p, b"");
    }
}

/// Start `pkexec oxtree PATH` with the session's display handed through,
/// since pkexec clears the environment. The call returns at once; the
/// receiver reports when the root instance ends.
pub fn relaunch_as_root(scan_path: &Path, config_dir: &Path) -> Result<Launch, String> {
    let exe = std::env::current_exe().map_err(|e| format!("Cannot find the OxTree binary: {e}"))?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let ready_file = runtime.join(format!("oxtree-root-{}", std::process::id()));
    let _ = std::fs::remove_file(&ready_file);
    if cfg!(windows) {
        return Err("OxTree already runs as administrator".into());
    }
    let mut cmd = if cfg!(target_os = "macos") {
        // `do shell script ... with administrator privileges` shows the
        // system's password prompt and waits for the command, as pkexec
        // does. The script is one shell line with every argument quoted.
        let line = [
            format!("OXTREE_CONFIG_DIR={}", config_dir.display()),
            format!("OXTREE_READY_FILE={}", ready_file.display()),
            exe.to_string_lossy().into_owned(),
            scan_path.to_string_lossy().into_owned(),
        ]
        .iter()
        .map(|a| oxtree_core::cleanup::quoted(a))
        .collect::<Vec<_>>()
        .join(" ");
        let mut cmd = Command::new("osascript");
        cmd.args(["-e", "on run argv"])
            .args([
                "-e",
                "do shell script (item 1 of argv) with administrator privileges",
            ])
            .args(["-e", "end run"])
            .arg(format!("env {line} >/dev/null 2>&1"));
        cmd
    } else {
        let mut cmd = Command::new("pkexec");
        cmd.arg("env");
        cmd
    };
    for key in [
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "XAUTHORITY",
        "XDG_SESSION_TYPE",
        "XDG_CURRENT_DESKTOP",
    ] {
        if let Ok(v) = std::env::var(key) {
            let v = if key == "WAYLAND_DISPLAY" && !v.starts_with('/') {
                // Root's runtime dir is not ours; give the socket's full path.
                match std::env::var("XDG_RUNTIME_DIR") {
                    Ok(dir) => format!("{dir}/{v}"),
                    Err(_) => v,
                }
            } else {
                v
            };
            cmd.arg(format!("{key}={v}"));
        }
    }
    if !cfg!(target_os = "macos") {
        cmd.arg(format!("OXTREE_CONFIG_DIR={}", config_dir.display()));
        cmd.arg(format!("OXTREE_READY_FILE={}", ready_file.display()));
        cmd.arg(exe).arg(scan_path);
    }
    // Nothing is piped: this process goes away once the root instance is
    // up, and a child writing to a closed pipe would fail its prints.
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Cannot ask for authorization: {e}"))?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = match child.wait() {
            Ok(status) => match status.code() {
                Some(0) => Ok(0),
                // osascript exits 1 for a cancelled prompt too.
                Some(1) if cfg!(target_os = "macos") => Err("Authorization dismissed.".into()),
                Some(126) => Err("Authorization dismissed.".into()),
                Some(127) => Err("Not authorized to run OxTree as root.".into()),
                Some(code) => Err(format!("The root instance ended with code {code}.")),
                None => Err("The root instance was killed.".into()),
            },
            Err(e) => Err(format!("Waiting for the root instance failed: {e}")),
        };
        let _ = tx.send(result);
    });
    Ok(Launch {
        exit: rx,
        ready_file,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn uid_is_read_from_proc() {
        let uid = effective_uid().unwrap();
        assert_eq!(uid == 0, is_root());
    }
}
