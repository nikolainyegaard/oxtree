//! File categories by name, with QDirStat's default table and matching
//! order: the longest suffix after the first dot first (case sensitive, then
//! case insensitive), shorter suffixes next, wildcard patterns last.
//! Executables and symlinks are categories of their own.

use std::collections::HashMap;

use crate::glob::glob_match;
use crate::tree::Kind;

pub const RED: [u8; 3] = [255, 0, 0];
pub const GREEN: [u8; 3] = [0, 255, 0];
pub const BLUE: [u8; 3] = [0, 0, 255];
pub const CYAN: [u8; 3] = [0, 255, 255];
pub const MAGENTA: [u8; 3] = [255, 0, 255];
pub const YELLOW: [u8; 3] = [255, 255, 0];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Category {
    pub name: String,
    pub color: [u8; 3],
}

pub struct Categorizer {
    categories: Vec<Category>,
    cs_suffix: HashMap<String, usize>,
    ci_suffix: HashMap<String, usize>,
    patterns: Vec<(String, bool, usize)>,
    executables: usize,
    symlinks: usize,
}

impl Default for Categorizer {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl Categorizer {
    pub fn empty() -> Categorizer {
        let mut c = Categorizer {
            categories: Vec::new(),
            cs_suffix: HashMap::new(),
            ci_suffix: HashMap::new(),
            patterns: Vec::new(),
            executables: 0,
            symlinks: 0,
        };
        c.executables = c.add("Executables", MAGENTA);
        c.symlinks = c.add("Symbolic Links", BLUE);
        c
    }

    pub fn with_defaults() -> Categorizer {
        let mut c = Categorizer::empty();

        let junk = c.add("Junk", RED);
        c.suffixes(junk, &["~", "bak"], false);
        c.pattern(junk, "core", true);

        let archives = c.add("Compressed Archives", GREEN);
        c.suffixes(
            archives,
            &[
                "7z", "arj", "cab", "cpio.gz", "deb", "fsa", "jar", "rar", "rpm", "tar.bz2",
                "tar.gz", "tar.lz", "tar.lzo", "tar.xz", "tar.zst", "tbz2", "tgz", "txz", "tz2",
                "tzst", "zip", "zpaq",
            ],
            false,
        );
        c.pattern(archives, "pack-*.pack", false);

        let plain_archives = c.add("Uncompressed Archives", [128, 128, 0]);
        c.suffixes(plain_archives, &["tar", "cpio"], false);

        let compressed = c.add("Compressed Files", GREEN);
        c.suffixes(compressed, &["bz2", "gz", "lz", "lzo", "xz", "zst"], false);

        let images = c.add("Images", CYAN);
        c.suffixes(
            images,
            &[
                "gif", "jpeg", "jpg", "jxl", "png", "mng", "svg", "tif", "tiff", "webp", "xcf.bz2",
                "xcf.gz", "xpm",
            ],
            false,
        );

        let plain_images = c.add("Uncompressed Images", RED);
        c.suffixes(
            plain_images,
            &["bmp", "pbm", "pgm", "pnm", "ppm", "xcf"],
            false,
        );

        let videos = c.add("Videos", [0xa0, 0xff, 0x00]);
        c.suffixes(
            videos,
            &[
                "asf", "avi", "divx", "dv", "flc", "fli", "flv", "m2ts", "m4v", "mk3d", "mkv",
                "mov", "mp2", "mp4", "mpeg", "mpg", "mts", "ogm", "ogv", "rm", "vdr", "vob",
                "webm", "wmp", "wmv",
            ],
            false,
        );

        let music = c.add("Music", YELLOW);
        c.suffixes(
            music,
            &[
                "aac", "aif", "ape", "caf", "dff", "dsf", "f4a", "f4b", "flac", "m4a", "m4b",
                "mid", "mka", "mp3", "oga", "ogg", "opus", "ra", "rax", "w64", "wav", "wma", "wv",
                "wvc",
            ],
            false,
        );

        let docs = c.add("Documents", BLUE);
        c.suffixes(
            docs,
            &[
                "doc", "docx", "dotx", "dvi", "dvi.bz2", "epub", "htm", "html", "ly", "md", "odb",
                "odc", "odg", "odp", "ods", "odt", "otc", "otp", "ots", "pdf", "potx", "ppsx",
                "ppt", "pptx", "ps", "sdc", "sdc.gz", "sdd", "sdp", "sdw", "sla", "sla.gz", "slaz",
                "sxi", "txt", "xls", "xlsx", "xlt", "css", "csv", "latex", "tex", "xml",
            ],
            false,
        );

        let src = c.add("Source Files", CYAN);
        c.suffixes(
            src,
            &[
                "c", "cpp", "cc", "cxx", "h", "hpp", "ui", "pl", "py", "rb", "el", "js", "php",
                "java", "pro", "cmake",
            ],
            true,
        );

        let obj = c.add("Object or Generated Files", [0xff, 0xa0, 0x00]);
        c.suffixes(
            obj,
            &["o", "lo", "ko", "Po", "al", "la", "moc", "elc", "pyc"],
            true,
        );
        for p in ["moc_*.cpp", "ui_*.cpp", "qrc_*.cpp"] {
            c.pattern(obj, p, true);
        }

        let libs = c.add("Libraries", [0xff, 0xa0, 0x00]);
        for p in ["lib*.so.*", "lib*.so", "lib*.a"] {
            c.pattern(libs, p, true);
        }
        c.suffixes(libs, &["dll", "so"], false);

        c
    }

    pub fn add(&mut self, name: &str, color: [u8; 3]) -> usize {
        self.categories.push(Category {
            name: name.into(),
            color,
        });
        self.categories.len() - 1
    }

    pub fn suffixes(&mut self, category: usize, suffixes: &[&str], case_sensitive: bool) {
        for s in suffixes {
            if case_sensitive {
                self.cs_suffix.insert((*s).to_string(), category);
            } else {
                self.ci_suffix.insert(s.to_lowercase(), category);
            }
        }
    }

    pub fn pattern(&mut self, category: usize, pattern: &str, case_sensitive: bool) {
        self.patterns
            .push((pattern.to_string(), case_sensitive, category));
    }

    pub fn categories(&self) -> &[Category] {
        &self.categories
    }

    /// The category of a node, given its name, kind and mode.
    pub fn category(&self, name: &str, kind: Kind, mode: u32) -> Option<&Category> {
        match kind {
            Kind::Symlink => self.categories.get(self.symlinks),
            Kind::File => match self.by_name(name) {
                Some(c) => Some(c),
                None if mode & 0o100 != 0 => self.categories.get(self.executables),
                None => None,
            },
            _ => None,
        }
    }

    /// The category a plain file name alone would get.
    pub fn by_name(&self, name: &str) -> Option<&Category> {
        if name.is_empty() {
            return None;
        }
        let mut suffix = name.split_once('.').map(|(_, rest)| rest).unwrap_or("");
        while !suffix.is_empty() {
            if let Some(&i) = self.cs_suffix.get(suffix) {
                return self.categories.get(i);
            }
            if let Some(&i) = self.ci_suffix.get(&suffix.to_lowercase()) {
                return self.categories.get(i);
            }
            suffix = suffix.split_once('.').map(|(_, rest)| rest).unwrap_or("");
        }
        self.patterns
            .iter()
            .find(|(p, cs, _)| glob_match(p, name, *cs))
            .and_then(|(_, _, i)| self.categories.get(*i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_table_matches_like_qdirstat() {
        let c = Categorizer::with_defaults();
        let name = |n: &str| c.by_name(n).map(|k| k.name.as_str());
        assert_eq!(name("foo.tar.gz"), Some("Compressed Archives"));
        assert_eq!(name("foo.gz"), Some("Compressed Files"));
        assert_eq!(name("Photo.JPG"), Some("Images"));
        assert_eq!(name("main.cpp"), Some("Source Files"));
        assert_eq!(name("MAIN.CPP"), None);
        assert_eq!(name("moc_main.cpp"), Some("Source Files"));
        assert_eq!(name("libz.so.1.3"), Some("Libraries"));
        assert_eq!(name("core"), Some("Junk"));
        assert_eq!(name("notes.bak"), Some("Junk"));
        assert_eq!(name("README"), None);
        assert_eq!(name("thing.unknownext"), None);
        assert_eq!(
            c.category("a.out", Kind::File, 0o100755)
                .map(|k| k.name.as_str()),
            Some("Executables")
        );
        assert_eq!(c.category("a.out", Kind::File, 0o100644), None);
        assert_eq!(
            c.category("link", Kind::Symlink, 0o120777)
                .map(|k| k.name.as_str()),
            Some("Symbolic Links")
        );
        assert_eq!(c.category("dir.tar", Kind::Dir, 0o040755), None);
    }
}
