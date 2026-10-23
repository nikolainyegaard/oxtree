//! User-defined cleanup actions: shell commands run on the selected item,
//! with QDirStat's placeholders, defaults and configuration keys.

use crate::ini::Ini;
use crate::tree::Kind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refresh {
    No,
    This,
    Parent,
    AssumeDeleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputPolicy {
    Always,
    IfErrorOutput,
    AfterTimeout,
    Never,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cleanup {
    pub title: String,
    pub command: String,
    pub active: bool,
    pub works_for_dir: bool,
    pub works_for_file: bool,
    pub works_for_dot_entry: bool,
    pub recurse: bool,
    pub ask_for_confirmation: bool,
    pub refresh: Refresh,
    pub output: OutputPolicy,
    pub shell: String,
    pub hotkey: String,
}

impl Cleanup {
    pub fn new(title: &str, command: &str) -> Cleanup {
        Cleanup {
            title: title.into(),
            command: command.into(),
            active: true,
            works_for_dir: true,
            works_for_file: true,
            works_for_dot_entry: true,
            recurse: false,
            ask_for_confirmation: false,
            refresh: Refresh::No,
            output: OutputPolicy::IfErrorOutput,
            shell: String::new(),
            hotkey: String::new(),
        }
    }

    /// The title without the `&` accelerator markers.
    pub fn display_title(&self) -> String {
        self.title
            .replace("&&", "\u{0}")
            .replace('&', "")
            .replace('\u{0}', "&")
    }

    pub fn works_for(&self, kind: Kind) -> bool {
        match kind {
            Kind::Dir | Kind::UnreadDir | Kind::MountPoint => self.works_for_dir,
            Kind::DotEntry => self.works_for_dot_entry,
            Kind::File | Kind::Symlink | Kind::Special => self.works_for_file,
        }
    }

    /// Expand `%p`, `%n`, `%d`, `%terminal` and `%filemanager`. `dir` is the
    /// directory the command runs in (the item for directories, the parent
    /// for files). `desktop` is `XDG_CURRENT_DESKTOP`.
    pub fn expand(&self, path: &str, name: &str, dir: &str, desktop: &str) -> String {
        let (terminal, file_manager) = desktop_apps(desktop);
        let mut cmd = self
            .command
            .replace("%terminal", terminal)
            .replace("%filemanager", file_manager);
        cmd = cmd
            .replace("%p", &quoted(path))
            .replace("%n", &quoted(name));
        if !dir.is_empty() {
            cmd = cmd.replace("%d", &quoted(dir));
        }
        cmd
    }
}

/// Single-quote a string for the shell. A quote inside becomes `'\''`, the
/// only way `sh` accepts one.
#[cfg(not(windows))]
pub fn quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Double-quote a string for `cmd`; a Windows path cannot contain `"`.
#[cfg(windows)]
pub fn quoted(s: &str) -> String {
    format!("\"{s}\"")
}

/// Terminal and file manager commands for a desktop, QDirStat's table.
pub fn desktop_apps(desktop: &str) -> (&'static str, &'static str) {
    if cfg!(target_os = "macos") {
        return ("open -a Terminal %d", "open");
    }
    if cfg!(windows) {
        return ("start \"\" cmd /K cd /d %d", "explorer");
    }
    let d = desktop.to_lowercase();
    if d.contains("kde") {
        ("konsole --workdir %d", "xdg-open")
    } else if d.contains("gnome") || d.contains("unity") {
        ("gnome-terminal &", "nautilus")
    } else if d.contains("xfce") {
        ("xfce4-terminal &", "thunar")
    } else if d.contains("lxde") {
        ("lxterminal &", "pcmanfm")
    } else if d.contains("enlightenment") {
        ("eterm &", "xdg-open")
    } else {
        ("xterm", "xdg-open")
    }
}

/// QDirStat's standard cleanups, in menu order.
pub fn standard() -> Vec<Cleanup> {
    let mut v = Vec::new();

    let mut c = Cleanup::new("Open File &Manager Here", "%filemanager %d &");
    c.output = OutputPolicy::Never;
    c.hotkey = "Ctrl+G".into();
    v.push(c);

    let mut c = Cleanup::new("Open &Terminal Here", "%terminal");
    c.output = OutputPolicy::Never;
    c.hotkey = "Ctrl+T".into();
    v.push(c);

    // The rest are shell commands; cmd has none of them. Trash covers
    // deleting on Windows.
    if cfg!(windows) {
        return v;
    }

    let mut c = Cleanup::new("Check File T&ype", "file %n | sed -e 's/[:,] /\\n  /g'");
    c.works_for_dir = false;
    c.works_for_dot_entry = false;
    c.output = OutputPolicy::Always;
    c.hotkey = "Ctrl+Y".into();
    v.push(c);

    let mut c = Cleanup::new("&Compress", "cd ..; tar cjvf %n.tar.bz2 %n && rm -rf %n");
    c.works_for_file = false;
    c.works_for_dot_entry = false;
    c.refresh = Refresh::Parent;
    v.push(c);

    let mut c = Cleanup::new("make cl&ean", "make clean");
    c.works_for_file = false;
    c.refresh = Refresh::This;
    v.push(c);

    let mut c = Cleanup::new("git cle&an", "git clean -dfx");
    c.works_for_file = false;
    c.ask_for_confirmation = true;
    c.refresh = Refresh::This;
    c.output = OutputPolicy::Always;
    v.push(c);

    let mut c = Cleanup::new("Delete J&unk Files", "rm -f *~ *.bak *.auto core");
    c.works_for_file = false;
    c.refresh = Refresh::This;
    c.recurse = true;
    c.shell = "/bin/bash".into();
    v.push(c);

    let mut c = Cleanup::new("&Delete (no way to undelete!)", "rm -rf %p");
    c.works_for_dot_entry = false;
    c.ask_for_confirmation = true;
    c.refresh = Refresh::Parent;
    c.hotkey = "Ctrl+Delete".into();
    v.push(c);

    let mut c = Cleanup::new("Clear Directory C&ontents", "rm -rf %d/*");
    c.works_for_file = false;
    c.works_for_dot_entry = false;
    c.ask_for_confirmation = true;
    c.refresh = Refresh::This;
    v.push(c);

    v
}

fn bool_of(s: Option<&str>, default: bool) -> bool {
    match s {
        Some("true") | Some("1") => true,
        Some("false") | Some("0") => false,
        _ => default,
    }
}

/// Read `[Cleanup_NN]` sections in QDirStat's layout. Sections without a
/// command are skipped.
pub fn from_ini(ini: &Ini) -> Vec<Cleanup> {
    let mut v = Vec::new();
    for (name, _) in &ini.sections {
        if !name.starts_with("Cleanup_") {
            continue;
        }
        let get = |k: &str| ini.get(name, k);
        let Some(command) = get("Command") else {
            continue;
        };
        let mut c = Cleanup::new(get("Title").unwrap_or(command), command);
        c.active = bool_of(get("Active"), true);
        c.works_for_dir = bool_of(get("WorksForDir"), true);
        c.works_for_file = bool_of(get("WorksForFile"), true);
        c.works_for_dot_entry = bool_of(get("WorksForDotEntry"), true);
        c.recurse = bool_of(get("Recurse"), false);
        c.ask_for_confirmation = bool_of(get("AskForConfirmation"), false);
        c.refresh = match get("RefreshPolicy") {
            Some("RefreshThis") => Refresh::This,
            Some("RefreshParent") => Refresh::Parent,
            Some("AssumeDeleted") => Refresh::AssumeDeleted,
            _ => Refresh::No,
        };
        c.output = match get("OutputWindowPolicy") {
            Some("ShowAlways") => OutputPolicy::Always,
            Some("ShowAfterTimeout") => OutputPolicy::AfterTimeout,
            Some("ShowNever") => OutputPolicy::Never,
            _ => OutputPolicy::IfErrorOutput,
        };
        c.shell = get("Shell").unwrap_or("").to_string();
        c.hotkey = get("Hotkey").unwrap_or("").to_string();
        v.push(c);
    }
    v
}

pub fn to_ini(cleanups: &[Cleanup]) -> Ini {
    let mut ini = Ini::default();
    for (i, c) in cleanups.iter().enumerate() {
        let refresh = match c.refresh {
            Refresh::No => "NoRefresh",
            Refresh::This => "RefreshThis",
            Refresh::Parent => "RefreshParent",
            Refresh::AssumeDeleted => "AssumeDeleted",
        };
        let output = match c.output {
            OutputPolicy::Always => "ShowAlways",
            OutputPolicy::IfErrorOutput => "ShowIfErrorOutput",
            OutputPolicy::AfterTimeout => "ShowAfterTimeout",
            OutputPolicy::Never => "ShowNever",
        };
        ini.add(
            &format!("Cleanup_{:02}", i + 1),
            &[
                ("Active", c.active.to_string()),
                ("AskForConfirmation", c.ask_for_confirmation.to_string()),
                ("Command", c.command.clone()),
                ("Hotkey", c.hotkey.clone()),
                ("OutputWindowPolicy", output.to_string()),
                ("Recurse", c.recurse.to_string()),
                ("RefreshPolicy", refresh.to_string()),
                ("Shell", c.shell.clone()),
                ("Title", c.title.clone()),
                ("WorksForDir", c.works_for_dir.to_string()),
                ("WorksForDotEntry", c.works_for_dot_entry.to_string()),
                ("WorksForFile", c.works_for_file.to_string()),
            ],
        );
    }
    ini
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expansion_quotes_and_escapes() {
        let c = Cleanup::new("x", "echo %p %n in %d");
        let s = c.expand("/tmp/Don't/do this", "do this", "/tmp/Don't", "KDE");
        assert_eq!(
            s,
            "echo '/tmp/Don'\\''t/do this' 'do this' in '/tmp/Don'\\''t'"
        );
        let t = Cleanup::new("t", "%terminal");
        assert_eq!(t.expand("/a", "a", "/", "KDE"), "konsole --workdir '/'");
        assert_eq!(t.expand("/a", "a", "/", "GNOME"), "gnome-terminal &");
        assert_eq!(t.expand("/a", "a", "/", ""), "xterm");
    }

    #[test]
    fn titles_and_kinds() {
        let std = standard();
        assert_eq!(std.len(), 9);
        assert_eq!(
            std.first().unwrap().display_title(),
            "Open File Manager Here"
        );
        assert!(
            std.iter()
                .any(|c| c.display_title() == "Delete (no way to undelete!)"
                    && c.ask_for_confirmation)
        );
        let check = std.iter().find(|c| c.command.starts_with("file ")).unwrap();
        assert!(check.works_for(Kind::File));
        assert!(!check.works_for(Kind::Dir));
        assert!(!check.works_for(Kind::DotEntry));
    }

    #[test]
    fn ini_roundtrip_and_qdirstat_keys() {
        let std = standard();
        let again = from_ini(&to_ini(&std));
        assert_eq!(again, std);
        let q = Ini::parse("[Cleanup_01]\nCommand=rm -rf %p\nTitle=&Delete\nAskForConfirmation=true\nRefreshPolicy=RefreshParent\nOutputWindowPolicy=ShowNever\nWorksForDotEntry=false\n[Other]\nx=1\n");
        let v = from_ini(&q);
        assert_eq!(v.len(), 1);
        let c = v.first().unwrap();
        assert!(
            c.ask_for_confirmation
                && c.refresh == Refresh::Parent
                && c.output == OutputPolicy::Never
                && !c.works_for_dot_entry
        );
    }
}
