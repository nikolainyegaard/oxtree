//! The opt-in update check: one GET of the GitHub releases API through
//! `curl` as a child process, never unless the user asked for it or turned
//! the automatic check on. Nothing is downloaded or installed.

use std::sync::mpsc::{self, Receiver};

use oxtree_core::settings::{parse_version, release_tag};

pub const RELEASES_URL: &str = "https://github.com/nikolainyegaard/oxtree/releases/latest";
const API_URL: &str = "https://api.github.com/repos/nikolainyegaard/oxtree/releases/latest";

/// What a check found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateResult {
    /// The latest tag, newer than this build.
    Newer(String),
    /// This build is the latest release, or newer than it.
    Current,
    Failed(String),
}

/// Start a check on a thread; the result arrives on the receiver.
pub fn check(current_version: &str) -> Receiver<UpdateResult> {
    let (tx, rx) = mpsc::channel();
    let current = current_version.to_string();
    std::thread::Builder::new()
        .name("oxtree-update".into())
        .spawn(move || {
            let _ = tx.send(check_blocking(&current));
        })
        .ok();
    rx
}

fn check_blocking(current: &str) -> UpdateResult {
    let out = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "15",
            "-A",
            &format!("oxtree/{current}"),
            "-H",
            "Accept: application/vnd.github+json",
            API_URL,
        ])
        .stdin(std::process::Stdio::null())
        .output();
    let out = match out {
        Ok(o) => o,
        Err(e) => return UpdateResult::Failed(format!("Cannot run curl: {e}")),
    };
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.lines().last().unwrap_or("").trim();
        return UpdateResult::Failed(if err.is_empty() {
            "The release check failed".into()
        } else {
            err.to_string()
        });
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let Some(tag) = release_tag(&text) else {
        return UpdateResult::Failed("No release tag in the response".into());
    };
    match (parse_version(&tag), parse_version(current)) {
        (Some(latest), Some(mine)) if latest > mine => UpdateResult::Newer(tag),
        (Some(_), Some(_)) => UpdateResult::Current,
        _ => UpdateResult::Failed(format!("Cannot read the version in {tag}")),
    }
}
