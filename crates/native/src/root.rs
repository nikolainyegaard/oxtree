//! Running as root, and starting a second instance as root through polkit.
//! Linux has no way to raise a running process's privileges, so "the full
//! picture" is always another process: `pkexec oxtree PATH`, as QDirStat
//! documents with sudo or pkexec. The root instance gets the invoking
//! user's display and config directory through the environment and never
//! writes configuration, so nothing ends up owned by root.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};

/// The effective user id from /proc/self/status, without libc.
pub fn effective_uid() -> Option<u32> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("Uid:"))?;
    line.split_whitespace().nth(2)?.parse().ok()
}

pub fn is_root() -> bool {
    effective_uid() == Some(0)
}

/// What the second instance reported: its exit code once it closed, or why
/// it could not start. pkexec exits 126 when the dialog is dismissed and
/// 127 when authorization is refused.
pub type LaunchResult = Result<i32, String>;

/// Start `pkexec oxtree PATH` with the session's display handed through,
/// since pkexec clears the environment. The call returns at once; the
/// receiver reports when the root instance ends.
pub fn relaunch_as_root(
    scan_path: &Path,
    config_dir: &Path,
) -> Result<Receiver<LaunchResult>, String> {
    let exe = std::env::current_exe().map_err(|e| format!("Cannot find the OxTree binary: {e}"))?;
    let mut cmd = Command::new("pkexec");
    cmd.arg("env");
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
    cmd.arg(format!("OXTREE_CONFIG_DIR={}", config_dir.display()));
    cmd.arg(exe).arg(scan_path);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("Cannot run pkexec: {e}"))?;
    let (tx, rx) = mpsc::channel();
    let stderr = child.stderr.take();
    std::thread::spawn(move || {
        let err = stderr
            .map(|mut s| {
                let mut buf = String::new();
                let _ = std::io::Read::read_to_string(&mut s, &mut buf);
                buf
            })
            .unwrap_or_default();
        let result = match child.wait() {
            Ok(status) => match status.code() {
                Some(0) => Ok(0),
                Some(126) => Err("Authorization dismissed.".into()),
                Some(127) => Err("Not authorized to run OxTree as root.".into()),
                Some(code) => {
                    let detail = err.trim();
                    if detail.is_empty() {
                        Err(format!("The root instance ended with code {code}."))
                    } else {
                        Err(format!(
                            "The root instance ended with code {code}: {detail}"
                        ))
                    }
                }
                None => Err("The root instance was killed.".into()),
            },
            Err(e) => Err(format!("Waiting for the root instance failed: {e}")),
        };
        let _ = tx.send(result);
    });
    Ok(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uid_is_read_from_proc() {
        let uid = effective_uid().unwrap();
        assert_eq!(uid == 0, is_root());
    }
}
