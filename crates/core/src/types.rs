//! The File Types view: every file grouped by its extension, with a count,
//! a total size and the category the extension falls in. WizTree's File
//! Types view and QDirStat's File Type Statistics are this table.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::mime::Categorizer;
use crate::tree::{NodeId, Tree};
use crate::Kind;

/// Shown for files whose name has no extension.
pub const NO_EXTENSION: &str = "<No Extension>";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeRow {
    /// Lower case, without the dot; `NO_EXTENSION` for none.
    pub ext: String,
    pub category: Option<String>,
    pub color: Option<[u8; 3]>,
    pub count: u32,
    pub size: u64,
}

impl TypeRow {
    /// The search box query that lists this type's files in the File View.
    pub fn query(&self) -> String {
        if self.ext == NO_EXTENSION {
            // A name with no dot after its first character.
            "/^\\.?[^.]+$".to_string()
        } else {
            format!("\"*.{}\"", self.ext)
        }
    }
}

/// The extension of a file name in lower case: `gz` for `a.tar.gz`, none
/// for `Makefile`, `.bashrc` and `archive.`.
pub fn extension(name: &str) -> Option<Cow<'_, str>> {
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() {
        return None;
    }
    Some(if ext.chars().any(|c| c.is_uppercase()) {
        Cow::Owned(ext.to_lowercase())
    } else {
        Cow::Borrowed(ext)
    })
}

/// One row per extension over the whole tree, largest total first.
pub fn file_types(tree: &Tree, categorizer: &Categorizer) -> Vec<TypeRow> {
    // ext -> (count, size, a node of that type for the category)
    let mut groups: HashMap<Cow<str>, (u32, u64, NodeId)> = HashMap::new();
    for n in 0..tree.len() as NodeId {
        if tree.is_dir_like(n) {
            continue;
        }
        let ext = extension(tree.name(n)).unwrap_or(Cow::Borrowed(NO_EXTENSION));
        let e = groups.entry(ext).or_insert((0, 0, n));
        e.0 = e.0.saturating_add(1);
        e.1 = e.1.saturating_add(tree.size(n));
        // A plain file stands for the type; a symlink would put the whole
        // extension under Symbolic Links.
        if tree.kind(e.2) != Kind::File && tree.kind(n) == Kind::File {
            e.2 = n;
        }
    }
    let mut rows: Vec<TypeRow> = groups
        .into_iter()
        .map(|(ext, (count, size, sample))| {
            let category =
                categorizer.category(tree.name(sample), tree.kind(sample), tree.mode(sample));
            TypeRow {
                ext: ext.into_owned(),
                category: category.map(|c| c.name.clone()),
                color: category.map(|c| c.color),
                count,
                size,
            }
        })
        .collect();
    rows.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.ext.cmp(&b.ext)));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::tests::sample;

    #[test]
    fn extensions_follow_the_last_dot_and_fold_case() {
        assert_eq!(extension("a.tar.gz").as_deref(), Some("gz"));
        assert_eq!(extension("PHOTO.JPG").as_deref(), Some("jpg"));
        assert_eq!(extension("Makefile"), None);
        assert_eq!(extension(".bashrc"), None);
        assert_eq!(extension("archive."), None);
        assert_eq!(extension(""), None);
    }

    #[test]
    fn groups_by_extension_largest_first() {
        let t = sample();
        let rows = file_types(&t, &Categorizer::with_defaults());
        let summary: Vec<(&str, u32, u64)> = rows
            .iter()
            .map(|r| (r.ext.as_str(), r.count, r.size))
            .collect();
        assert_eq!(
            summary,
            vec![(NO_EXTENSION, 1, 1000), ("bin", 2, 350), ("txt", 1, 100)]
        );
        assert_eq!(rows[2].category.as_deref(), Some("Documents"));
        assert_eq!(rows[0].query(), "/^\\.?[^.]+$");
        assert_eq!(rows[1].query(), "\"*.bin\"");
    }
}
