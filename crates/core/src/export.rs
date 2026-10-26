//! Export: the tree as CSV the way WizTree writes it, and as indented text
//! for the clipboard.

use crate::time;
use crate::tree::{NodeId, Tree};
use crate::{format, Kind};

#[derive(Clone, Debug)]
pub struct ExportOptions {
    /// `,` for CSV, `\t` for TSV.
    pub separator: char,
    /// Directories deeper than this below the export root are left out,
    /// with everything under them; `None` exports everything.
    pub max_depth: Option<u32>,
    /// Added to the timestamps; 0 writes UTC.
    pub utc_offset: i32,
    pub percent_of_parent: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions {
            separator: ',',
            max_depth: None,
            utc_offset: 0,
            percent_of_parent: false,
        }
    }
}

/// WizTree's columns: File Name, Size, Allocated, Modified, Attributes,
/// Files, Folders, plus % of Parent when asked. Directories end in `/`
/// and come before their contents. `keep` drops single rows (a search
/// filter); a dropped directory still has its contents exported.
pub fn csv(
    tree: &Tree,
    root: NodeId,
    opts: &ExportOptions,
    keep: impl Fn(NodeId) -> bool,
) -> String {
    let sep = opts.separator;
    let mut out = String::new();
    let mut header = vec![
        "File Name",
        "Size",
        "Allocated",
        "Modified",
        "Attributes",
        "Files",
        "Folders",
    ];
    if opts.percent_of_parent {
        header.push("% of Parent");
    }
    push_row(&mut out, sep, header.iter().map(|s| s.to_string()));
    let root_depth = tree.depth(root);
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let kind = tree.kind(n);
        if kind == Kind::DotEntry {
            for c in tree.children(n).rev() {
                stack.push(c);
            }
            continue;
        }
        let depth = tree.depth(n) - root_depth;
        if opts.max_depth.is_some_and(|m| depth > m) {
            continue;
        }
        if keep(n) {
            let mut path = tree.path(n);
            if tree.is_dir_like(n) && !path.ends_with('/') {
                path.push('/');
            }
            let size = tree.size(n).to_string();
            let mut fields = vec![
                path,
                size.clone(),
                size,
                time::format(tree.mtime(n), opts.utc_offset),
                format::permissions(tree.mode(n)),
            ];
            if tree.is_dir_like(n) {
                fields.push(tree.total_files(n).to_string());
                fields.push(tree.total_subdirs(n).to_string());
            } else {
                fields.push(String::new());
                fields.push(String::new());
            }
            if opts.percent_of_parent {
                fields.push(
                    tree.percent(n)
                        .map(|p| format!("{p:.1}"))
                        .unwrap_or_default(),
                );
            }
            push_row(&mut out, sep, fields.into_iter());
        }
        for c in tree.children(n).rev() {
            stack.push(c);
        }
    }
    out
}

fn push_row(out: &mut String, sep: char, fields: impl Iterator<Item = String>) {
    for (i, f) in fields.enumerate() {
        if i > 0 {
            out.push(sep);
        }
        if f.contains(sep) || f.contains('"') || f.contains('\n') {
            out.push('"');
            out.push_str(&f.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(&f);
        }
    }
    out.push('\n');
}

/// The rows as the tree shows them, name and size, indented two spaces per
/// level: WizTree's "copy to clipboard" of the visible tree.
pub fn tree_text(tree: &Tree, rows: impl Iterator<Item = (NodeId, u32)>) -> String {
    let mut out = String::new();
    for (n, depth) in rows {
        for _ in 0..depth {
            out.push_str("  ");
        }
        out.push_str(tree.name(n));
        if tree.is_dir_like(n) && tree.kind(n) != Kind::DotEntry && !tree.name(n).ends_with('/') {
            out.push('/');
        }
        out.push('\t');
        out.push_str(&format::size(tree.size(n)));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::tests::sample;

    #[test]
    fn csv_lists_directories_before_contents_and_quotes_separators() {
        let t = sample();
        let text = csv(&t, t.root(), &ExportOptions::default(), |_| true);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0],
            "File Name,Size,Allocated,Modified,Attributes,Files,Folders"
        );
        assert!(lines[1].starts_with("/root/,"), "{}", lines[1]);
        assert!(lines[1].ends_with(",rwxr-xr-x,4,2"), "{}", lines[1]);
        assert!(lines[2].starts_with("/root/big/,"), "{}", lines[2]);
        assert!(lines[3].starts_with("/root/big/d,1000,1000,1970-01-01 00:00,rw-r--r--,,"));
        assert_eq!(lines.len(), 8, "{text}");

        let opts = ExportOptions {
            separator: '\t',
            max_depth: Some(0),
            percent_of_parent: true,
            ..Default::default()
        };
        let text = csv(&t, t.root(), &opts, |_| true);
        assert_eq!(text.lines().count(), 2);
        assert!(text.lines().nth(1).unwrap().ends_with("\t4\t2\t"));

        let text = csv(&t, t.root(), &ExportOptions::default(), |n| {
            t.name(n) == "d"
        });
        assert_eq!(text.lines().count(), 2);
    }

    #[test]
    fn quoting() {
        let mut out = String::new();
        push_row(
            &mut out,
            ',',
            ["a,b".to_string(), "say \"hi\"".to_string()].into_iter(),
        );
        assert_eq!(out, "\"a,b\",\"say \"\"hi\"\"\"\n");
    }

    #[test]
    fn tree_text_indents() {
        let t = sample();
        let big = t.children(0).next().unwrap();
        let d = t.children(big).next().unwrap();
        let text = tree_text(&t, [(0, 0), (big, 1), (d, 2)].into_iter());
        format::set_size_units(format::SizeUnits::Binary);
        assert_eq!(
            text,
            "/root/\t13.4 KiB\n  big/\t5.0 KiB\n    d\t1000 Bytes\n"
        );
    }
}
