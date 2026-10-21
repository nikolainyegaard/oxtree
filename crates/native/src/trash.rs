//! Move files to the desktop trash through `gio trash`, which implements the
//! freedesktop trash specification for every desktop. Running it keeps the
//! implementation and its edge cases (other filesystems, trash directories
//! per mount) out of this binary.

use std::process::Command;

pub fn move_to_trash(path: &str) -> Result<(), String> {
    let output = Command::new("gio")
        .arg("trash")
        .arg("--")
        .arg(path)
        .output()
        .map_err(|e| format!("Cannot run gio: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if err.is_empty() {
        format!("gio trash failed for {path}")
    } else {
        err
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_an_error_not_a_panic() {
        if Command::new("gio").arg("version").output().is_err() {
            return;
        }
        let err = move_to_trash("/definitely/not/here/oxtree").unwrap_err();
        assert!(!err.is_empty());
    }
}
