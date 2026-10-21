//! A small INI reader and writer compatible with the files QSettings
//! writes, so QDirStat's cleanup configuration can be read as is.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ini {
    pub sections: Vec<(String, Vec<(String, String)>)>,
}

impl Ini {
    pub fn parse(text: &str) -> Ini {
        let mut ini = Ini::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                ini.sections.push((name.trim().to_string(), Vec::new()));
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if ini.sections.is_empty() {
                ini.sections.push((String::new(), Vec::new()));
            }
            if let Some(section) = ini.sections.last_mut() {
                section
                    .1
                    .push((key.trim().to_string(), unquote(value.trim())));
            }
        }
        ini
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .iter()
            .find(|(n, _)| n == section)
            .and_then(|(_, kv)| kv.iter().find(|(k, _)| k == key))
            .map(|(_, v)| v.as_str())
    }

    pub fn section(&self, name: &str) -> Option<&[(String, String)]> {
        self.sections
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, kv)| kv.as_slice())
    }

    pub fn add(&mut self, section: &str, pairs: &[(&str, String)]) {
        self.sections.push((
            section.to_string(),
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.clone()))
                .collect(),
        ));
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (name, kv) in &self.sections {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("[{name}]\n"));
            for (k, v) in kv {
                out.push_str(&format!("{k}={}\n", quote(v)));
            }
        }
        out
    }
}

fn unquote(v: &str) -> String {
    let inner = match v.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        Some(inner) => inner,
        None => return v.to_string(),
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

fn quote(v: &str) -> String {
    let needs = v.is_empty()
        || v.starts_with(' ')
        || v.ends_with(' ')
        || v.chars()
            .any(|c| matches!(c, '"' | '\\' | '\n' | '\t' | '\r' | ';' | '#' | '='));
    if !needs {
        return v.to_string();
    }
    let mut out = String::from("\"");
    for c in v.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_qsettings_style_values() {
        let text = "[Cleanup_03]\nCommand=\"file %n | sed -e 's/[:,] /\\\\n  /g'\"\nTitle=Check File T&ype\n\n[Cleanup_08]\nCommand=rm -rf %p\n";
        let ini = Ini::parse(text);
        assert_eq!(
            ini.get("Cleanup_03", "Command"),
            Some("file %n | sed -e 's/[:,] /\\n  /g'")
        );
        assert_eq!(ini.get("Cleanup_03", "Title"), Some("Check File T&ype"));
        assert_eq!(ini.get("Cleanup_08", "Command"), Some("rm -rf %p"));
        assert_eq!(ini.get("Cleanup_08", "Nope"), None);
    }

    #[test]
    fn roundtrip() {
        let mut ini = Ini::default();
        ini.add(
            "S",
            &[
                ("a", "plain".into()),
                ("b", "with \"quotes\" and \\ back\nnewline".into()),
                ("c", String::new()),
            ],
        );
        let again = Ini::parse(&ini.to_text());
        assert_eq!(again, ini);
    }
}
