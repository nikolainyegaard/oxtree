//! Move files to the desktop trash through `gio trash`, which implements the
//! freedesktop trash specification for every desktop; through Finder on
//! macOS and the Recycle Bin on Windows. Running a helper keeps the
//! implementation and its edge cases (other filesystems, trash directories
//! per mount) out of this binary.

use std::process::Command;

pub fn move_to_trash(path: &str) -> Result<(), String> {
    let output = if cfg!(target_os = "macos") {
        Command::new("osascript")
            .args(["-e", "on run argv"])
            .args([
                "-e",
                "tell application \"Finder\" to delete (POSIX file (item 1 of argv) as alias)",
            ])
            .args(["-e", "end run"])
            .arg(path)
            .output()
            .map_err(|e| format!("Cannot run osascript: {e}"))?
    } else if cfg!(windows) {
        Command::new("powershell")
            .args(["-NoProfile", "-Command"])
            .arg("Add-Type -AssemblyName Microsoft.VisualBasic; $p = $env:OXTREE_PATH; if (Test-Path -LiteralPath $p -PathType Container) { [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($p, 'OnlyErrorDialogs', 'SendToRecycleBin') } else { [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($p, 'OnlyErrorDialogs', 'SendToRecycleBin') }")
            .env("OXTREE_PATH", path)
            .output()
            .map_err(|e| format!("Cannot run powershell: {e}"))?
    } else {
        Command::new("gio")
            .arg("trash")
            .arg("--")
            .arg(path)
            .output()
            .map_err(|e| format!("Cannot run gio: {e}"))?
    };
    if output.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if err.is_empty() {
        format!("Moving {path} to the trash failed")
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
