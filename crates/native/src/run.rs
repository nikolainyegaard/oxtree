//! Run a cleanup command in a shell with its output streamed back line by
//! line, the way QDirStat's OutputWindow shows it.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};

pub enum RunEvent {
    Stdout(String),
    Stderr(String),
    /// Exit status; `None` when the process was killed by a signal.
    Finished(Option<i32>),
    Failed(String),
}

/// The shell for cleanups: the one the cleanup names, else `$SHELL`, else
/// bash, else sh. Only executables are accepted.
pub fn choose_shell(preferred: &str) -> Option<String> {
    let mut candidates: Vec<String> = Vec::new();
    if !preferred.is_empty() {
        candidates.push(preferred.to_string());
    }
    if let Ok(s) = std::env::var("SHELL") {
        candidates.push(s);
    }
    candidates.push("/bin/bash".into());
    candidates.push("/bin/sh".into());
    candidates
        .into_iter()
        .find(|c| std::fs::metadata(c).map(|m| m.is_file()).unwrap_or(false))
}

/// Start `shell -c command` in `dir`. Events arrive on the channel from a
/// reader thread; the caller drains it once per frame.
pub fn run(shell: &str, command: &str, dir: &str) -> Receiver<RunEvent> {
    let (tx, rx) = mpsc::channel();
    let child = Command::new(shell)
        .arg("-c")
        .arg(command)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(RunEvent::Failed(format!("Cannot start {shell}: {e}")));
            return rx;
        }
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let tx_err = tx.clone();
    let err_thread = std::thread::spawn(move || {
        if let Some(err) = stderr {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                let _ = tx_err.send(RunEvent::Stderr(line));
            }
        }
    });
    std::thread::spawn(move || {
        if let Some(out) = stdout {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                let _ = tx.send(RunEvent::Stdout(line));
            }
        }
        let _ = err_thread.join();
        match child.wait() {
            Ok(status) => {
                let _ = tx.send(RunEvent::Finished(status.code()));
            }
            Err(e) => {
                let _ = tx.send(RunEvent::Failed(format!(
                    "Waiting for the command failed: {e}"
                )));
            }
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_both_outputs_and_exit_code() {
        let shell = choose_shell("").unwrap();
        let rx = run(&shell, "echo out; echo err 1>&2; exit 3", "/");
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut code = None;
        for ev in rx {
            match ev {
                RunEvent::Stdout(l) => out.push(l),
                RunEvent::Stderr(l) => err.push(l),
                RunEvent::Finished(c) => code = c,
                RunEvent::Failed(e) => panic!("{e}"),
            }
        }
        assert_eq!(out, vec!["out"]);
        assert_eq!(err, vec!["err"]);
        assert_eq!(code, Some(3));
        assert!(choose_shell("/nonexistent/shell").is_some());
    }
}
