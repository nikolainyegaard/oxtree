//! The opt-in update check and the Update Now that follows it. The check is
//! one GET of the GitHub releases API through `curl`, never unless the user
//! asked for it or turned the automatic check on. Updating is a second
//! explicit press, and then this copy is replaced the way it was installed:
//!
//! | Installed as | Update |
//! |---|---|
//! | a package from a repository | `dnf upgrade` or `apt-get install` of the package, as root through polkit |
//! | a .rpm or .deb from the release page | download the new one, check its SHA-256, `dnf install` or `apt-get install` it as root through polkit |
//! | an AppImage | download the new one, check it, rename it over this one |
//! | anything else (the tarball, a development build) | the link |
//!
//! `pkexec` rather than `sudo`: it shows the desktop's own prompt and
//! follows the session's polkit rules, as Rescan as Root does. The package
//! manager does every install, so its database stays right.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};

use oxtree_core::settings::{parse_version, release_tag};

pub const RELEASES_URL: &str = "https://github.com/nikolainyegaard/oxtree/releases/latest";
const API_URL: &str = "https://api.github.com/repos/nikolainyegaard/oxtree/releases/latest";
const DOWNLOAD_URL: &str = "https://github.com/nikolainyegaard/oxtree/releases/download";
const PACKAGE: &str = "oxtree";

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
    let out = Command::new("curl")
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
        .stdin(Stdio::null())
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

/// How this running copy was installed, which decides how it updates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Install {
    /// From a repository the package manager knows; it has the update.
    Repo(Manager),
    /// A package installed from a file; the next file goes the same way.
    Package(Manager),
    /// The AppImage file.
    AppImage(PathBuf),
    /// The tarball, a development build: nothing to do by hand but the link.
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Manager {
    Dnf,
    Apt,
}

impl Install {
    pub fn label(&self) -> String {
        match self {
            Install::Repo(Manager::Dnf) => "installed from a dnf repository".into(),
            Install::Repo(Manager::Apt) => "installed from an apt repository".into(),
            Install::Package(Manager::Dnf) => "installed from an .rpm file".into(),
            Install::Package(Manager::Apt) => "installed from a .deb file".into(),
            Install::AppImage(p) => format!("the AppImage at {}", p.display()),
            Install::Other => "not installed by a package manager".into(),
        }
    }

    pub fn can_update(&self) -> bool {
        !matches!(self, Install::Other)
    }
}

/// Detect the install. Runs the package managers' query commands, a few
/// milliseconds; call it once.
pub fn detect() -> Install {
    if let Some(p) = std::env::var_os("APPIMAGE") {
        return Install::AppImage(PathBuf::from(p));
    }
    if !has("pkexec") {
        return Install::Other;
    }
    if has("rpm") && capture("rpm", &["-q", PACKAGE]).is_some() {
        let from = capture(
            "dnf",
            &[
                "repoquery",
                "--installed",
                "--queryformat",
                "%{from_repo}\n",
                PACKAGE,
            ],
        )
        .unwrap_or_default();
        return if repo_from_dnf(&from) {
            Install::Repo(Manager::Dnf)
        } else {
            Install::Package(Manager::Dnf)
        };
    }
    if has("dpkg-query")
        && capture("dpkg-query", &["-W", "-f", "${db:Status-Status}", PACKAGE])
            .is_some_and(|s| s.trim() == "installed")
    {
        let policy = capture("apt-cache", &["policy", PACKAGE]).unwrap_or_default();
        return if repo_from_apt(&policy) {
            Install::Repo(Manager::Apt)
        } else {
            Install::Package(Manager::Apt)
        };
    }
    Install::Other
}

/// `dnf repoquery --installed --queryformat %{from_repo}`: `@commandline`
/// for a file install, `@System` on dnf4 for one it cannot tell, else the
/// repository id.
fn repo_from_dnf(from_repo: &str) -> bool {
    let r = from_repo.trim();
    !(r.is_empty() || r == "@commandline" || r == "@System")
}

/// `apt-cache policy`: a repository install lists the installed version
/// with a source line beyond `/var/lib/dpkg/status`.
fn repo_from_apt(policy: &str) -> bool {
    // Source lines are `PRIORITY URL suite/component arch Packages`.
    policy.lines().any(|l| {
        let mut f = l.split_whitespace();
        f.next().is_some_and(|p| p.parse::<u32>().is_ok())
            && f.next()
                .is_some_and(|src| src.contains("://") || src.starts_with("file:"))
    })
}

fn has(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|d| d.join(cmd).is_file()))
}

fn capture(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What the Update Now thread reports: progress lines, then the outcome.
/// `Done(Ok(exe))` is the program to start in place of this one.
#[derive(Debug)]
pub enum UpdateEvent {
    Progress(String),
    Done(Result<PathBuf, String>),
}

/// Update to `tag` on a thread. The caller starts the returned program
/// and quits when `Done(Ok)` arrives.
pub fn update(install: Install, tag: String, cache_dir: PathBuf) -> Receiver<UpdateEvent> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("oxtree-update".into())
        .spawn(move || {
            let result = update_blocking(&install, &tag, &cache_dir, &tx);
            let _ = tx.send(UpdateEvent::Done(result));
        })
        .ok();
    rx
}

fn update_blocking(
    install: &Install,
    tag: &str,
    cache_dir: &Path,
    tx: &Sender<UpdateEvent>,
) -> Result<PathBuf, String> {
    let say = |s: &str| {
        let _ = tx.send(UpdateEvent::Progress(s.to_string()));
    };
    let version = tag.trim_start_matches('v');
    let arch = std::env::consts::ARCH;
    match install {
        Install::Repo(Manager::Dnf) => {
            say("Asking dnf for the update...");
            run_as_root(&["dnf", "upgrade", "-y", "--refresh", PACKAGE])?;
            installed_binary()
        }
        Install::Repo(Manager::Apt) => {
            say("Asking apt for the update...");
            run_as_root(&[
                "sh",
                "-c",
                &format!("apt-get update && apt-get install -y {PACKAGE}"),
            ])?;
            installed_binary()
        }
        Install::Package(manager) => {
            let name = match manager {
                Manager::Dnf => format!("{PACKAGE}-{version}-1.{arch}.rpm"),
                Manager::Apt => format!(
                    "{PACKAGE}_{version}-1_{}.deb",
                    match arch {
                        "x86_64" => "amd64",
                        "aarch64" => "arm64",
                        other => other,
                    }
                ),
            };
            let file = download_verified(tag, &name, cache_dir, &say)?;
            say("Installing the package...");
            let path = file.to_string_lossy().into_owned();
            let result = match manager {
                Manager::Dnf => run_as_root(&["dnf", "install", "-y", &path]),
                Manager::Apt => run_as_root(&["apt-get", "install", "-y", &path]),
            };
            let _ = std::fs::remove_file(&file);
            result?;
            installed_binary()
        }
        Install::AppImage(current) => {
            let name = format!("OxTree-{version}-{arch}.AppImage");
            let file = download_verified(tag, &name, cache_dir, &say)?;
            say("Replacing the AppImage...");
            let dir = current
                .parent()
                .ok_or("The AppImage is not in a directory")?;
            let staged = dir.join(format!(".{name}.new"));
            std::fs::copy(&file, &staged)
                .map_err(|e| format!("Cannot write next to the AppImage: {e}"))?;
            let _ = std::fs::remove_file(&file);
            #[cfg(unix)]
            let _ = std::fs::set_permissions(
                &staged,
                std::os::unix::fs::PermissionsExt::from_mode(0o755),
            );
            // The running copy is mounted from its own file and keeps
            // working after the rename.
            std::fs::rename(&staged, current)
                .map_err(|e| format!("Cannot replace the AppImage: {e}"))?;
            Ok(current.clone())
        }
        Install::Other => Err("This copy cannot update itself".into()),
    }
}

/// The package's binary after an install, for the relaunch.
fn installed_binary() -> Result<PathBuf, String> {
    let p = PathBuf::from("/usr/bin/oxtree");
    if p.is_file() {
        Ok(p)
    } else {
        std::env::current_exe().map_err(|e| format!("Cannot find the new OxTree: {e}"))
    }
}

/// Download one release file and its SHA256SUMS into the cache directory
/// and verify the file with `sha256sum`; the file's path on success.
fn download_verified(
    tag: &str,
    name: &str,
    cache_dir: &Path,
    say: &dyn Fn(&str),
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(cache_dir)
        .map_err(|e| format!("Cannot create {}: {e}", cache_dir.display()))?;
    say(&format!("Downloading {name}..."));
    let file = cache_dir.join(name);
    let sums = cache_dir.join("SHA256SUMS");
    for (url_name, dest) in [(name, &file), ("SHA256SUMS", &sums)] {
        let url = format!("{DOWNLOAD_URL}/{tag}/{url_name}");
        let status = Command::new("curl")
            .args(["-fsSL", "--max-time", "600", "-o"])
            .arg(dest)
            .arg(&url)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| format!("Cannot run curl: {e}"))?;
        if !status.success() {
            let _ = std::fs::remove_file(&file);
            let _ = std::fs::remove_file(&sums);
            return Err(format!(
                "Download of {url_name} failed (curl exit {})",
                status.code().unwrap_or(-1)
            ));
        }
    }
    say("Checking the download...");
    let ok = Command::new("sha256sum")
        .args(["--check", "--ignore-missing", "--status", "SHA256SUMS"])
        .current_dir(cache_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Cannot run sha256sum: {e}"))?
        .success();
    let _ = std::fs::remove_file(&sums);
    if !ok {
        let _ = std::fs::remove_file(&file);
        return Err(format!("{name} did not match the release's checksum"));
    }
    Ok(file)
}

/// Run a command as root through polkit, which shows the desktop's prompt.
fn run_as_root(args: &[&str]) -> Result<(), String> {
    let status = Command::new("pkexec")
        .args(args)
        .stdin(Stdio::null())
        .status()
        .map_err(|e| format!("Cannot run pkexec: {e}"))?;
    match status.code() {
        Some(0) => Ok(()),
        Some(126) => Err("Authorization dismissed".into()),
        Some(127) => Err("Not authorized".into()),
        Some(c) => Err(format!("The package manager stopped with exit code {c}")),
        None => Err("The package manager was interrupted".into()),
    }
}

/// Start the new program detached, so this one can close.
pub fn relaunch(exe: &Path, args: &[String]) -> Result<(), String> {
    Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Cannot start {}: {e}", exe.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_installs_are_told_apart_from_file_installs() {
        assert!(!repo_from_dnf("@commandline\n"));
        assert!(!repo_from_dnf("@System"));
        assert!(!repo_from_dnf(""));
        assert!(repo_from_dnf(
            "copr:copr.fedorainfracloud.org:nikolainyegaard:oxtree\n"
        ));

        let file_install = "oxtree:\n  Installed: 0.1.0-1\n  Candidate: 0.1.0-1\n  Version table:\n *** 0.1.0-1 100\n        100 /var/lib/dpkg/status\n";
        assert!(!repo_from_apt(file_install));
        let repo_install = "oxtree:\n  Installed: 0.1.0-1\n  Candidate: 0.1.1-1\n  Version table:\n     0.1.1-1 500\n        500 https://example.org/apt stable/main amd64 Packages\n *** 0.1.0-1 500\n        500 https://example.org/apt stable/main amd64 Packages\n        100 /var/lib/dpkg/status\n";
        assert!(repo_from_apt(repo_install));
    }

    #[test]
    fn labels_and_updatability() {
        assert!(Install::Repo(Manager::Dnf).can_update());
        assert!(Install::AppImage(PathBuf::from("/x")).can_update());
        assert!(!Install::Other.can_update());
        assert_eq!(
            Install::Package(Manager::Apt).label(),
            "installed from a .deb file"
        );
    }
}
