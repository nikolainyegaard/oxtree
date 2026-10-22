//! The settings file: every choice that survives a restart, as an INI with
//! QDirStat's section and key names where the two agree. Missing or
//! unreadable values fall back to their defaults one by one, so an edited
//! file never stops the app.

use crate::dupes::DupMode;
use crate::ini::Ini;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// `[DirectoryTree] CrossFilesystems`
    pub cross_filesystems: bool,
    /// `[Treemaps] RebuildDelayMillisec`
    pub rebuild_delay_ms: u64,
    /// `[Treemaps] TileNames`
    pub tile_names: bool,
    /// `[MainWindow] ShowTreemap`, `ShowDetailsPanel`
    pub show_treemap: bool,
    pub show_details: bool,
    /// `[MainWindow] Width`, `Height`; none until the window was closed once
    pub window: Option<(f32, f32)>,
    /// `[MainWindow] Theme`: system, light or dark
    pub theme: Theme,
    /// `[General] SizeDecimals`, 0 to 3
    pub size_decimals: u8,
    /// `[TreeView] RowHeight`, 16 to 40
    pub row_height: f32,
    /// `[TreeView] Columns`, `[FileView] Columns`, `[FileTypes] Columns`
    pub tree_columns: Vec<f32>,
    pub file_columns: Vec<f32>,
    pub types_columns: Vec<f32>,
    /// `[FileView] MaxFiles`, `Folders`, `Duplicates`, `MatchPath`
    pub file_max: usize,
    pub file_folders: bool,
    pub dup_mode: DupMode,
    pub match_path: bool,
    /// `[FileView] History0` to `History9`, newest first
    pub search_history: Vec<String>,
    /// `[Updates] CheckAutomatically`, `CheckIntervalDays`, `LastCheck`
    pub check_updates: bool,
    pub check_interval_days: u32,
    pub last_check: i64,
}

pub const HISTORY_MAX: usize = 10;

impl Default for Settings {
    fn default() -> Self {
        Settings {
            cross_filesystems: true,
            rebuild_delay_ms: 0,
            tile_names: true,
            show_treemap: true,
            show_details: true,
            window: None,
            theme: Theme::System,
            size_decimals: 1,
            row_height: 20.0,
            tree_columns: Vec::new(),
            file_columns: Vec::new(),
            types_columns: Vec::new(),
            file_max: 1_000,
            file_folders: false,
            dup_mode: DupMode::Off,
            match_path: false,
            search_history: Vec::new(),
            check_updates: false,
            check_interval_days: 7,
            last_check: 0,
        }
    }
}

fn flag(ini: &Ini, section: &str, key: &str, default: bool) -> bool {
    match ini.get(section, key) {
        Some("true") | Some("1") => true,
        Some("false") | Some("0") => false,
        _ => default,
    }
}

fn number<T: std::str::FromStr + PartialOrd>(
    ini: &Ini,
    section: &str,
    key: &str,
    default: T,
    min: T,
    max: T,
) -> T {
    match ini.get(section, key).and_then(|v| v.trim().parse().ok()) {
        Some(v) if v >= min && v <= max => v,
        _ => default,
    }
}

fn floats(ini: &Ini, section: &str, key: &str) -> Vec<f32> {
    ini.get(section, key)
        .map(|v| {
            v.split(',')
                .filter_map(|x| x.trim().parse::<f32>().ok())
                .filter(|x| x.is_finite() && *x >= 0.0)
                .collect()
        })
        .unwrap_or_default()
}

fn join(v: &[f32]) -> String {
    v.iter()
        .map(|x| format!("{x:.0}"))
        .collect::<Vec<_>>()
        .join(",")
}

impl Settings {
    pub fn from_ini(ini: &Ini) -> Settings {
        let d = Settings::default();
        let window = match (
            ini.get("MainWindow", "Width")
                .and_then(|v| v.parse::<f32>().ok()),
            ini.get("MainWindow", "Height")
                .and_then(|v| v.parse::<f32>().ok()),
        ) {
            (Some(w), Some(h)) if w >= 320.0 && h >= 200.0 && w <= 16_384.0 && h <= 16_384.0 => {
                Some((w, h))
            }
            _ => None,
        };
        let mut history: Vec<String> = (0..HISTORY_MAX)
            .filter_map(|i| ini.get("FileView", &format!("History{i}")))
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .collect();
        history.dedup();
        Settings {
            cross_filesystems: flag(
                ini,
                "DirectoryTree",
                "CrossFilesystems",
                d.cross_filesystems,
            ),
            rebuild_delay_ms: number(ini, "Treemaps", "RebuildDelayMillisec", 0, 0, 10_000),
            tile_names: flag(ini, "Treemaps", "TileNames", d.tile_names),
            show_treemap: flag(ini, "MainWindow", "ShowTreemap", true),
            show_details: flag(ini, "MainWindow", "ShowDetailsPanel", true),
            window,
            theme: match ini.get("MainWindow", "Theme") {
                Some("light") => Theme::Light,
                Some("dark") => Theme::Dark,
                _ => Theme::System,
            },
            size_decimals: number(ini, "General", "SizeDecimals", 1, 0, 3),
            row_height: number(ini, "TreeView", "RowHeight", 20.0, 16.0, 40.0),
            tree_columns: floats(ini, "TreeView", "Columns"),
            file_columns: floats(ini, "FileView", "Columns"),
            types_columns: floats(ini, "FileTypes", "Columns"),
            file_max: number(ini, "FileView", "MaxFiles", 1_000, 0, 100_000_000),
            file_folders: flag(ini, "FileView", "Folders", false),
            dup_mode: match ini.get("FileView", "Duplicates") {
                Some("name-size") => DupMode::NameSize,
                Some("name-size-date") => DupMode::NameSizeDate,
                _ => DupMode::Off,
            },
            match_path: flag(ini, "FileView", "MatchPath", false),
            search_history: history,
            check_updates: flag(ini, "Updates", "CheckAutomatically", false),
            check_interval_days: number(ini, "Updates", "CheckIntervalDays", 7, 1, 365),
            last_check: number(ini, "Updates", "LastCheck", 0, 0, i64::MAX),
        }
    }

    pub fn to_ini(&self) -> Ini {
        let mut ini = Ini::default();
        ini.add(
            "General",
            &[("SizeDecimals", self.size_decimals.to_string())],
        );
        let mut main = vec![
            ("ShowTreemap", self.show_treemap.to_string()),
            ("ShowDetailsPanel", self.show_details.to_string()),
            (
                "Theme",
                match self.theme {
                    Theme::System => "system",
                    Theme::Light => "light",
                    Theme::Dark => "dark",
                }
                .to_string(),
            ),
        ];
        if let Some((w, h)) = self.window {
            main.push(("Width", format!("{w:.0}")));
            main.push(("Height", format!("{h:.0}")));
        }
        ini.add("MainWindow", &main);
        ini.add(
            "DirectoryTree",
            &[("CrossFilesystems", self.cross_filesystems.to_string())],
        );
        ini.add(
            "Treemaps",
            &[
                ("RebuildDelayMillisec", self.rebuild_delay_ms.to_string()),
                ("TileNames", self.tile_names.to_string()),
            ],
        );
        ini.add(
            "TreeView",
            &[
                ("RowHeight", format!("{:.0}", self.row_height)),
                ("Columns", join(&self.tree_columns)),
            ],
        );
        let mut fv = vec![
            ("MaxFiles", self.file_max.to_string()),
            ("Folders", self.file_folders.to_string()),
            (
                "Duplicates",
                match self.dup_mode {
                    DupMode::Off => "off",
                    DupMode::NameSize => "name-size",
                    DupMode::NameSizeDate => "name-size-date",
                }
                .to_string(),
            ),
            ("MatchPath", self.match_path.to_string()),
            ("Columns", join(&self.file_columns)),
        ];
        let keys: Vec<String> = (0..HISTORY_MAX).map(|i| format!("History{i}")).collect();
        for (k, h) in keys
            .iter()
            .zip(self.search_history.iter().take(HISTORY_MAX))
        {
            fv.push((k.as_str(), h.clone()));
        }
        ini.add("FileView", &fv);
        ini.add("FileTypes", &[("Columns", join(&self.types_columns))]);
        ini.add(
            "Updates",
            &[
                ("CheckAutomatically", self.check_updates.to_string()),
                ("CheckIntervalDays", self.check_interval_days.to_string()),
                ("LastCheck", self.last_check.to_string()),
            ],
        );
        ini
    }

    /// Put a query at the front of the history, once.
    pub fn remember_search(&mut self, query: &str) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        self.search_history.retain(|h| h != q);
        self.search_history.insert(0, q.to_string());
        self.search_history.truncate(HISTORY_MAX);
    }
}

/// `vX.Y.Z` or `X.Y.Z` as a tuple, for comparing release tags.
pub fn parse_version(tag: &str) -> Option<(u64, u64, u64)> {
    let mut it = tag
        .trim()
        .trim_start_matches('v')
        .split('.')
        .map(|p| p.parse::<u64>());
    Some((it.next()?.ok()?, it.next()?.ok()?, it.next()?.ok()?))
}

/// The `tag_name` of a GitHub releases API response, without a JSON parser.
pub fn release_tag(json: &str) -> Option<String> {
    let i = json.find("\"tag_name\"")?;
    let rest = json.get(i + "\"tag_name\"".len()..)?;
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest.get(..end)?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_defaults_for_garbage() {
        let mut s = Settings {
            cross_filesystems: false,
            rebuild_delay_ms: 200,
            tile_names: false,
            show_treemap: false,
            show_details: true,
            window: Some((1280.0, 720.0)),
            theme: Theme::Dark,
            size_decimals: 2,
            row_height: 24.0,
            tree_columns: vec![400.0, 188.0],
            file_columns: vec![520.0],
            types_columns: vec![],
            file_max: 0,
            file_folders: true,
            dup_mode: DupMode::NameSizeDate,
            match_path: true,
            search_history: vec!["*.mp3 | *.flac".into(), "say \"hi\"".into()],
            check_updates: true,
            check_interval_days: 30,
            last_check: 1_700_000_000,
        };
        let text = s.to_ini().to_text();
        assert!(
            text.contains("[Treemaps]\nRebuildDelayMillisec=200"),
            "{text}"
        );
        assert_eq!(Settings::from_ini(&Ini::parse(&text)), s);

        s.remember_search("  new  ");
        s.remember_search("*.mp3 | *.flac");
        assert_eq!(s.search_history[0], "*.mp3 | *.flac");
        assert_eq!(s.search_history[1], "new");
        assert_eq!(s.search_history.len(), 3);

        let bad = Ini::parse(
            "[General]\nSizeDecimals=9\n[TreeView]\nRowHeight=abc\nColumns=1,x,-5,30\n[MainWindow]\nWidth=10\nHeight=10\nTheme=purple\n[Updates]\nCheckIntervalDays=0\n",
        );
        let s = Settings::from_ini(&bad);
        assert_eq!(s.size_decimals, 1);
        assert_eq!(s.row_height, 20.0);
        assert_eq!(s.tree_columns, vec![1.0, 30.0]);
        assert_eq!(s.window, None);
        assert_eq!(s.theme, Theme::System);
        assert_eq!(s.check_interval_days, 7);
        assert_eq!(Settings::from_ini(&Ini::default()), Settings::default());
    }

    #[test]
    fn versions_and_release_tags() {
        assert_eq!(parse_version("v0.2.0"), Some((0, 2, 0)));
        assert_eq!(parse_version("1.10.3"), Some((1, 10, 3)));
        assert_eq!(parse_version("v1.2"), None);
        assert_eq!(parse_version("latest"), None);
        let json = "{\"url\": \"x\", \"tag_name\": \"v0.3.1\", \"name\": \"v0.3.1\"}";
        assert_eq!(release_tag(json).as_deref(), Some("v0.3.1"));
        assert_eq!(release_tag("{}"), None);
        assert!(parse_version("v0.3.1") > parse_version("v0.1.0"));
    }
}
