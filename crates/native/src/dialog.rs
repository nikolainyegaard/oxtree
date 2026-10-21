//! A directory chooser through the desktop's own dialog program, run as a
//! child process on a thread: kdialog on KDE, zenity elsewhere. When
//! neither exists the result is `None` and the caller falls back to its
//! text field.

use std::process::Command;
use std::sync::mpsc::{self, Receiver};

pub fn pick_directory(start: &str, desktop: &str) -> Receiver<Option<String>> {
    let (tx, rx) = mpsc::channel();
    let start = start.to_string();
    let kde = desktop.to_lowercase().contains("kde");
    std::thread::spawn(move || {
        let kdialog = || {
            Command::new("kdialog")
                .arg("--getexistingdirectory")
                .arg(&start)
                .arg("--title")
                .arg("Select Directory")
                .output()
        };
        let zenity = || {
            Command::new("zenity")
                .arg("--file-selection")
                .arg("--directory")
                .arg("--title=Select Directory")
                .arg(format!("--filename={start}/"))
                .output()
        };
        let out = if kde {
            kdialog().or_else(|_| zenity())
        } else {
            zenity().or_else(|_| kdialog())
        };
        let picked = match out {
            Ok(o) if o.status.success() => {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            }
            _ => None,
        };
        let _ = tx.send(picked);
    });
    rx
}
